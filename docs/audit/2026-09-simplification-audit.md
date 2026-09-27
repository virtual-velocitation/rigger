# Simplification Audit - 2026-09

The audit report spec 85 derives the follow-up refactoring specs from. Six sections, each claim citing `file:line`; see `specs/85-simplification-audit.md` for scope.

## 1. Responsibility Map

Every function in `src/conductor.rs`, `src/main.rs` and `src/dash.rs` (1688 functions total), assigned to a proposed module by `tests/simplification_audit.rs`'s deterministic scanner + rule-table classifier (never by hand). Instrument: the brace-matching scanner over the three named files.

### Proposed module tree

- `conductor::agent_failure` (3 functions)
  - `src/conductor.rs:1839-1854` `as_str` - method inside `impl AgentFailure`; grouped with its other `AgentFailure` methods.
  - `src/conductor.rs:1863-1868` `from_category` - method inside `impl AgentFailure`; grouped with its other `AgentFailure` methods.
  - `src/conductor.rs:1872-1874` `fmt` - method inside `impl std::fmt::Display for AgentFailure`; grouped with its other `AgentFailure` methods.
- `conductor::budget` (4 functions)
  - `src/conductor.rs:355-357` `compensation_queued_key` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:968-997` `pending_compensations_from_log` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:1623-1625` `is_budget_refused` - name contains "budget" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:13378-13388` `mutation_scratch_settled` - name contains "mutation" (budget accounting); grouped under `conductor::budget`.
- `conductor::emit` (2 functions)
  - `src/conductor.rs:385-387` `quarantine_record_key` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
  - `src/conductor.rs:13170-13203` `recorded_adoption` - name contains "record" (event/decision emission); grouped under `conductor::emit`.
- `conductor::gate` (19 functions)
  - `src/conductor.rs:341-348` `gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:395-402` `gate_intersects_radius` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:565-567` `unit_of_gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:575-579` `gate_key_attempt` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:638-668` `recorded_gate_outcome` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:673-675` `deferred_gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:684-686` `deferred_gate_failed_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:1718-1720` `is_verdict_channel_mismatch` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11722-11729` `with_gate_hold` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11737-11745` `union_gates` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11859-11866` `gate_failure_cause` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11878-11880` `verdict_approves` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11896-11905` `last_verdict` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11912-11914` `has_verdict_line` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11921-11923` `emitted_verdict_approves` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:11933-11945` `verdict_compensates` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:13481-13487` `ungated_fan_out_templates` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:13616-13624` `critique_gate_name` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:13803-13848` `assert_no_ungated_fanout_unit` - name contains "gate" (gate execution); grouped under `conductor::gate`.
- `conductor::gate_ratchet` (1 function)
  - `src/conductor.rs:869-875` `for_persistent_failure` - method inside `impl GateRatchet`; grouped with its other `GateRatchet` methods.
- `conductor::ground` (1 function)
  - `src/conductor.rs:12458-12467` `graph_around_recovery` - name contains "graph" (grounding integration); grouped under `conductor::ground`.
- `conductor::integration_approval` (1 function)
  - `src/conductor.rs:1280-1282` `approved` - method inside `impl IntegrationApproval`; grouped with its other `IntegrationApproval` methods.
- `conductor::prior_failure` (3 functions)
  - `src/conductor.rs:1306-1311` `is_empty` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `src/conductor.rs:1315-1333` `summary` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `src/conductor.rs:1338-1385` `block` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
- `conductor::review` (11 functions)
  - `src/conductor.rs:307-309` `review_round_start_key` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:504-551` `route_review_tier` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:1683-1696` `degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:1701-1703` `is_degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:11815-11824` `review_evidence` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:12028-12035` `review_protocol` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:13131-13145` `recorded_review_round_start_sha` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:13255-13260` `review_worktree_dir` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:13267-13269` `review_branch` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:13722-13724` `review_roster` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:13730-13736` `adjudicator_roster` - name contains "adjudicat" (review orchestration); grouped under `conductor::review`.
- `conductor::review_outcome` (2 functions)
  - `src/conductor.rs:911-919` `approved` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `src/conductor.rs:920-928` `rejected` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
- `conductor::run` (6 functions)
  - `src/conductor.rs:12853-12855` `unit_branch` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:12873-12879` `unit_worktree_dir` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:13533-13546` `stale_units_from_log` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:13568-13606` `stale_downstream_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:13631-13649` `unit_slug` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:13659-13698` `baseline_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
- `conductor::run_ctx` (125 functions)
  - `src/conductor.rs:3264-3266` `emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3275-3290` `append_and_fold` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3302-3324` `append_and_fold_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3329-3335` `emit_with_actor` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3345-3353` `emit_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3366-3368` `emit_keyed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3379-3404` `emit_keyed_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3439-3478` `emit_keyed_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3487-3493` `agent_model` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3500-3502` `is_stale` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3527-3538` `effective_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3549-3557` `gate_verdict_high_water` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3565-3579` `mark_stale_downstream` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3595-3673` `drain_compensations` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3686-3735` `commits_to_compensate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3745-3776` `emit_gate_skip` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3793-3852` `emit_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3860-3867` `max_retries` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3880-3891` `max_retries_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3917-3923` `budget_tripped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3930-3932` `spawn_is_recorded` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3948-3971` `reserve_spawn` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3980-4010` `trip_budget_breaker` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4021-4031` `halt_reason` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4036-4046` `check_coverage_or_flag` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4048-4228` `run_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4237-4271` `partition_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4276-4286` `partition_requested` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4291-4315` `run_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4317-4370` `start_and_run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4372-4480` `run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4485-4502` `stage_paused_for_review` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4510-4512` `effective_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4524-4531` `select_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4543-4565` `log_review_tier` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4579-4597` `assert_isolated_cwd` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4612-4673` `reviewer_spawn_opts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4710-4725` `review_round_start_sha` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4795-4956` `review_unit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4985-5039` `guard_review_round_tree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5066-5084` `guard_review_round_tree_on_tier_err` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5121-5182` `spawn_sdet_author` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5193-5217` `halted_spawn_checkpoint_permitted` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5223-5989` `run_single_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5995-6002` `effective_speculation_width` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6011-6018` `speculates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6027-6043` `speculation_lane_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6064-6432` `run_speculation` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6445-6536` `emit_speculation_winner_status` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6555-6592` `record_speculation_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6602-6635` `cancel_speculation_candidates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6637-6690` `run_fan_out_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6700-6866` `run_fan_out_review_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6897-6949` `run_review_agents_concurrently` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6961-6992` `run_lens` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7031-7186` `run_reviewer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7222-7239` `gating_spawn_emitted_approve` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7261-7271` `review_spawn_errored` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7295-7328` `reviewer_result_is_degenerate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7343-7373` `run_adversary` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7391-7436` `run_adjudicator` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7458-7479` `dag_unit_blast_radii` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7488-7567` `build_dag_critique_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7576-7682` `re_plan` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7693-7726` `run_plan_critique_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7736-7902` `plan_critique_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7930-7941` `build_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7958-7965` `run_base_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7990-7996` `spawn_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8007-8012` `build_budget` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8037-8045` `shared_build_cache_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8056-8312` `run_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8328-8419` `run_gate_with_taxonomy` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8476-8483` `gc_integrated_branches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8490-8593` `gc_integrated_branches_logged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8629-8636` `reclaim_terminal_unit_mutation_scratch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8666-8852` `run_deferred_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8863-8930` `record_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8988-9557` `integrate_and_emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9599-9606` `integrate_plan_commits` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9608-9728` `integrate_plan_commits_inner` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9739-9755` `record_plan_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9766-9794` `read_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9802-9831` `record_plan_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9836-9842` `regenerate_rule_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9867-9904` `run_regenerate_command` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9923-9948` `regenerate_conflicted_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:9976-9998` `catch_up_owed_regeneration` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10003-10009` `regenerate_pending_for` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10016-10024` `union_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10051-10079` `record_regenerate_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10088-10102` `record_placeholder_staged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10112-10129` `record_regenerate_commit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10139-10153` `record_merge_attempt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10159-10178` `record_merge_outcome` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10185-10200` `record_landing_intent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10210-10225` `record_landed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10236-10254` `record_integrate_row` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10268-10330` `spawn_conflict_resolution_implementer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10340-10342` `build_system_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10352-10364` `ground_query` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10370-10380` `implementer_agent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10384-10408` `plan_protocol` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10436-10470` `grounded_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10486-10499` `grounded_seed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10523-10552` `record_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10554-10560` `build_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10571-10573` `build_review_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10581-10615` `build_prompt_with_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10617-10681` `graph_context` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10694-10702` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10710-10769` `ingest_project_batches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10774-10774` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10788-10796` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10800-10800` `ingest_files_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10802-10817` `emit_lesson` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10830-10871` `land_refused` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:10876-10882` `agent_isolated` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:11005-11113` `adopt_prior_criterion_branch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:11135-11176` `resume_phase` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:11178-11210` `stage_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:11225-11247` `review_only_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:11259-11264` `template_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:11266-11667` `harvest_proposed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:11689-11711` `resolve_served_criterion` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
- `conductor::schedule` (8 functions)
  - `src/conductor.rs:1751-1757` `plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:1762-1764` `is_plan_landing_failed` - name contains "plan" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:11769-11771` `normalize_criterion_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:11790-11795` `criterion_stable_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:12977-13037` `prior_criterion_unit` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:13407-13427` `blast_radius_conflicts` - name contains "blast" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:13453-13461` `first_stage_named` - name contains "stage" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:13939-13958` `ready_stages` - name contains "stage" (unit scheduling); grouped under `conductor::schedule`.
- `conductor::spawn` (5 functions)
  - `src/conductor.rs:1541-1545` `parked_spawn` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1550-1552` `is_parked` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1603-1618` `spawn_halt` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1639-1641` `is_parked_or_budget_refused` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:13860-13871` `wave_ready` - name contains "wave" (spawn lifecycle); grouped under `conductor::spawn`.
- `conductor::support` (22 functions)
  - `src/conductor.rs:446-448` `path_is_high_risk` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:1054-1087` `conflict_regenerate_pending_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:1153-1175` `pending_landing_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:1190-1215` `landed_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:1225-1256` `integrate_attempted_from_log` - name contains "from_" (conversion helper); grouped under `conductor::support`.
  - `src/conductor.rs:1785-1787` `is_land_refused` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:2877-2879` `has_producer` - name contains "has_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:11754-11756` `normalize_ws` - name contains "normalize" (normalization helper); grouped under `conductor::support`.
  - `src/conductor.rs:11998-12003` `build_system_prompt` - name contains "build" (generic construction helper); grouped under `conductor::support`.
  - `src/conductor.rs:12274-12418` `write_capped_section` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12469-12525` `write_code_neighborhood` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12638-12721` `write_design_intent` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12727-12742` `write_capped_decisions` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12747-12764` `write_capped_lessons` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12772-12797` `write_capped_findings` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12806-12814` `write_peers_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:12829-12839` `write_lookup_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:13089-13102` `branch_is_foreign` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:13439-13441` `is_fan_out` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:13468-13470` `is_fan_out_template` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:13490-13492` `is_producer` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:13742-13744` `has_llm_verifier` - name contains "has_" (predicate helper); grouped under `conductor::support`.
- `conductor::tests` (491 functions)
  - `src/conductor.rs:3209-3260` `for_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14096-14100` `agent_failure_as_str_round_trips_through_from_category_for_every_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14103-14108` `agent_failure_from_category_degrades_an_unrecognized_string_to_unknown` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14111-14113` `agent_failure_default_is_unknown` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14116-14121` `classify_failure_prefers_the_stopfailure_record_over_api_retry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14124-14129` `classify_failure_falls_back_to_the_last_api_retry_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14132-14134` `classify_failure_falls_back_to_unknown_when_neither_source_has_anything` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14137-14151` `strip_failure_marker_drops_the_class_prefix_leaving_only_the_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14154-14157` `strip_failure_marker_passes_through_an_unmarked_error_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14160-14179` `unit_worktree_dir_derives_deterministically_from_scratch_root_and_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14182-14272` `recorded_gate_outcome_reads_the_latest_gate_run_verdict_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14185-14197` `verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14276-14281` `started_with_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14283-14288` `integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14291-14299` `prior_criterion_unit_finds_a_prior_un_integrated_units_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14302-14328` `prior_criterion_unit_never_returns_an_integrated_units_id_and_never_falls_back_to_an_older_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14331-14355` `prior_criterion_unit_integration_of_one_criterion_never_masks_an_abandoned_sibling_criterion_sharing_the_same_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14358-14371` `prior_criterion_unit_tie_break_prefers_the_most_recent_of_two_non_integrated_priors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14374-14379` `prior_criterion_unit_excludes_this_unit_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14382-14394` `prior_criterion_unit_ignores_a_different_criterion_and_an_empty_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14401-14406` `run_started_with_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14411-14417` `compensated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14422-14427` `plain_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14430-14451` `prior_criterion_unit_never_adopts_across_two_different_specs_sharing_the_same_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14454-14469` `prior_criterion_unit_still_adopts_across_two_runs_of_the_same_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14472-14490` `prior_criterion_unit_readopts_after_a_compensation_reverts_the_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14493-14510` `prior_criterion_unit_a_plain_non_compensation_failure_never_reopens_an_integrated_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14517-14535` `adoption_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14538-14563` `recorded_adoption_ignores_a_same_identity_event_carrying_the_wrong_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14566-14599` `recorded_adoption_never_answers_for_a_mismatched_criterion_or_a_mismatched_spec_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14602-14607` `branch_owner_returns_none_for_an_id_that_never_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14610-14626` `branch_owner_reads_the_most_recent_started_criterion_and_spec_for_this_bare_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14629-14644` `branch_owner_ignores_a_non_unit_started_event_even_when_it_shares_the_id_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14647-14660` `branch_is_foreign_is_false_when_nothing_is_recorded_or_everything_matches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14663-14685` `branch_is_foreign_is_false_when_the_recorded_owner_has_no_criterion_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14688-14710` `branch_is_foreign_when_only_one_axis_differs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14715-14727` `find_unit_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14730-14816` `a_fresh_units_own_branch_adopts_a_prior_runs_un_integrated_unit_sharing_the_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14819-14884` `a_fresh_unit_never_adopts_a_criterion_whose_prior_attempt_already_integrated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14887-15040` `a_halted_spawns_uncommitted_tree_is_captured_as_a_wip_commit_and_named_in_the_next_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15043-15121` `a_dirty_tree_whose_named_spawn_was_never_requested_gets_no_wip_recovery_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15124-15233` `a_dirty_tree_gets_no_wip_recovery_commit_while_a_sibling_spawn_of_the_unit_is_still_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15236-15329` `a_prior_runs_leftover_spawn_request_for_a_same_named_unit_never_halts_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15341-15356` `prior_failure_summary_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15359-15382` `prior_failure_block_names_only_the_halted_commit_when_it_is_the_sole_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15385-15419` `prior_failure_block_adds_the_generic_preamble_for_review_reject_or_contradiction_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15422-15449` `review_worktree_dir_and_branch_derive_from_stage_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15561-15589` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15592-15599` `prompts_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15602-15609` `dirs_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15613-15619` `system_prompt_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15623-15625` `title_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15629-15631` `reviews_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15635-15637` `spawn_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15641-15647` `spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15653-15660` `dir_existed_when_spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15663-15803` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15807-15833` `integrates_a_passing_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15836-15860` `coverage_gate_refuses_an_uncovered_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15863-15895` `planner_extends_the_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15898-15964` `planner_proposed_unit_with_a_coverage_criterion_runs_and_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15967-16055` `conductor_creates_one_baseline_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16058-16129` `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16132-16254` `producer_prompt_carries_the_criteria_and_plan_protocol_grounded_on_the_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16259-16261` `baseline_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16266-16272` `supersede_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16275-16324` `a_prior_runs_proposal_never_resurrects_in_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16327-16409` `planner_unit_supersedes_the_matching_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16412-16496` `a_planner_supersede_of_a_fan_out_member_still_satisfies_its_downstream_needs_edge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16499-16535` `a_criterion_with_no_planner_unit_keeps_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16538-16611` `planner_refinement_split_is_still_harvested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16614-16747` `a_same_id_re_emit_updates_the_proposed_unit_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16754-16784` `seed_refine_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16788-16808` `append_proposals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16811-16884` `a_coverage_retarget_re_emit_preserves_one_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16887-16947` `a_re_emit_naming_a_reserved_stage_never_mutates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16950-17015` `re_folding_a_same_id_re_emit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17018-17125` `a_real_split_two_distinct_ids_both_survive_the_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17128-17224` `a_later_episodes_proposal_supersedes_an_earlier_episodes_planner_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17227-17284` `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17287-17366` `a_planner_proposal_with_no_data_episode_field_still_supersedes_via_meta_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17369-17496` `a_same_id_refine_restamps_its_episode_so_its_own_episodes_sibling_does_not_reap_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17499-17621` `a_same_id_refine_survives_its_own_episodes_sibling_add_walked_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17624-17765` `a_resume_catch_up_over_two_episode_supersession_and_a_split_matches_a_live_incremental_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17658-17676` `append_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17768-17925` `a_legacy_history_resume_catch_up_matches_a_live_incremental_fold_mutual_siblings_then_superseded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17789-17806` `append_legacy` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17808-17826` `append_identified` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17928-18022` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18025-18124` `a_late_re_emit_never_mutates_a_started_or_terminal_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18127-18244` `harvest_proposed_gates_every_case_with_the_templates_list_unioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18247-18327` `harvest_proposed_gate_inheritance_survives_a_resumed_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18330-18347` `plan_protocol_tells_the_planner_gates_come_from_the_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18353-18362` `has_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18365-18421` `a_verbatim_copy_still_supersedes_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18424-18498` `a_paraphrased_proposal_matches_its_baseline_by_id_not_prose` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18501-18585` `a_bracketed_id_echo_with_a_paraphrase_still_supersedes_exactly_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18588-18660` `a_stale_non_matching_id_falls_back_to_the_verbatim_prose_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18663-18721` `a_genuinely_new_proposal_runs_and_records_an_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18724-18789` `a_genuinely_new_proposal_with_no_gates_still_spawns_gated_via_template_inheritance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18792-18923` `resume_dedups_baselines_before_running_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18926-19007` `decomposes_the_real_spec_01_into_per_criterion_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19010-19048` `ratchet_promotes_a_reliable_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19051-19112` `elevated_gate_is_never_promoted_to_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19115-19157` `learns_from_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19160-19210` `feeds_graph_decisions_into_the_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19213-19292` `lookup_pointer_names_all_three_verbs_on_every_slice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19295-19390` `decisions_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19395-19417` `render_capped_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19420-19450` `a_superseded_decision_never_outranks_a_current_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19453-19506` `grounding_still_surfaces_a_prior_run_decision_that_peers_labels_historical` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19509-19536` `the_verbatim_count_cap_binds_on_many_small_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19539-19571` `the_byte_budget_cap_binds_before_the_count_on_chunky_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19574-19641` `a_kept_decision_restores_the_dropped_dependency_it_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19647-19667` `render_capped_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19670-19737` `findings_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19740-19775` `the_verbatim_count_cap_binds_on_many_small_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19778-19883` `grounding_omits_a_resolved_finding_and_keeps_the_open_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19889-19908` `render_capped_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19911-19968` `the_injected_slice_is_deduplicated_by_normalized_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19971-20113` `dedup_and_restore_are_render_only_no_event_no_projection_mutation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20131-20267` `structural_grounding_is_one_seeded_traversal_over_the_unified_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20284-20399` `the_grounding_path_populates_the_unified_graph_from_the_live_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20414-20503` `re_ingesting_re_extracts_a_changed_file_and_skips_unchanged_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20514-20597` `ingest_files_into_graph_is_bounded_to_the_named_files_and_reflects_their_live_content` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20618-20698` `re_excluding_the_same_file_twice_in_one_process_retires_its_middle_generation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20711-20830` `a_second_run_over_an_unchanged_tree_appends_no_derived_index_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20848-20866` `spec60_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20878-20909` `spec60_reachable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20915-20924` `spec60_reached_entities` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20939-21118` `editing_one_file_between_runs_re_emits_only_that_files_batch_and_supersedes_its_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21131-21287` `a_file_reverted_to_an_earlier_recorded_generation_re_ingests_and_matches_a_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21311-21444` `a_prior_runs_non_ingest_replay_key_never_suppresses_this_runs_keyed_emit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21456-21545` `the_ingest_sink_appends_and_folds_once_per_file_batch_never_once_per_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21466-21474` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21568-21727` `design_intent_grounding_renders_the_governing_rule_and_specifying_ra_by_traversal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21730-21800` `a_governing_decision_never_leaks_into_the_design_intent_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21803-21859` `the_design_intent_section_renders_the_newest_binding_and_elides_the_oldest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21862-21902` `a_subgraph_with_no_design_intent_renders_no_design_intent_header` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21909-21935` `render_code_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21938-21974` `the_code_neighborhood_elision_note_names_graph_around_recovery_not_peers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21977-22022` `the_code_neighborhood_byte_cap_elides_before_the_count_cap_is_reached` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22025-22055` `a_subgraph_with_no_code_definitions_renders_no_code_neighborhood_header` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22058-22124` `lessons_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22127-22164` `the_verbatim_count_cap_binds_on_many_small_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22170-22192` `render_capped_lessons_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22195-22232` `lessons_rank_by_blast_radius_relevance_over_pure_recency` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22235-22275` `resume_skips_already_integrated_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22279-22296` `seed_events_in_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22299-22353` `replay_trajectory_keeps_only_the_world_inputs_and_restrips_the_run_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22358-22375` `commit_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22378-22473` `resume_reuses_a_units_branch_instead_of_reimplementing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22476-22570` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22578-22594` `commit_on_named_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22609-22665` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22668-22733` `branch_gc_reclaims_the_recorded_branch_not_the_derived_name_when_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22744-22768` `seed_lingering_worktree_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22774-22784` `worktree_registered_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22787-22884` `branch_gc_removes_a_lingering_worktree_before_reclaiming_the_branch_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22887-22979` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22982-23063` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23066-23133` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23136-23235` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23238-23321` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23324-23427` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23430-23481` `review_boundary_events_carry_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23484-23533` `a_review_reject_unitfailed_carries_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23536-23580` `an_exhaustive_gate_failure_on_an_approved_unit_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23583-23708` `stamps_the_model_alias_and_resolved_id_on_live_lifecycle_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23714-23730` `degenerate_reviewer_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23733-23802` `a_degenerate_adjudicator_result_respawns_and_a_substantive_retry_folds_normally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23805-23916` `a_review_stage_error_result_re_parks_a_fresh_attempt_no_charge_then_a_real_verdict_folds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23919-24053` `a_review_spawn_that_errors_on_every_re_parked_attempt_escalates_through_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24056-24149` `a_gating_spawn_that_emits_an_approve_verdict_but_returns_no_verdict_line_hard_errors_with_the_result_channel_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24152-24197` `a_gating_spawn_that_returns_a_reject_verdict_line_is_a_normal_reject_even_if_it_emitted_approve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24200-24244` `a_gating_spawn_with_no_verdict_line_and_no_emitted_approve_is_an_ordinary_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24247-24353` `the_workflow_live_path_correlates_the_approve_by_stamp_not_a_bare_stream_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24356-24409` `a_degenerate_lens_result_respawns_the_lens_before_the_review_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24412-24472` `a_lens_that_emitted_a_finding_but_reports_empty_stdout_is_not_degenerate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24475-24572` `a_reviewer_that_only_ever_returns_degenerate_output_halts_the_run_loudly_naming_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24575-24660` `resume_integrates_an_already_approved_unit_without_re_reviewing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24663-24774` `a_failed_unit_is_not_terminal_and_resumes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24777-24892` `remediation_attempts_accumulate_across_resume_and_escalate_at_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24895-24980` `an_escalated_unit_stays_terminal_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24983-25038` `a_fresh_unit_with_no_branch_runs_the_full_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25041-25096` `agent_decision_folds_content_but_no_agent_attribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25099-25141` `scope_creep_refuses_a_criterionless_proposed_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25144-25177` `adjudicator_reject_blocks_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25180-25246` `adversary_runs_between_the_lenses_and_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25249-25299` `adjudicator_reject_gates_even_with_an_adversary_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25302-25395` `unit_reviews_itself_within_its_own_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25398-25408` `depth_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25412-25419` `full_panel_with_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25421-25423` `strs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25426-25452` `path_is_high_risk_matches_by_prefix_and_by_glob` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25455-25464` `route_review_tier_with_no_policy_is_the_full_panel_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25467-25486` `route_review_tier_routes_a_low_risk_unit_to_the_light_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25489-25504` `route_review_tier_forces_full_on_a_high_risk_path_hit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25507-25520` `route_review_tier_forces_full_over_the_size_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25523-25536` `route_review_tier_forces_full_when_the_gates_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25539-25573` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25579-25631` `run_tiered_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25635-25646` `logged_review_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25649-25675` `a_low_risk_unit_runs_the_light_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25678-25738` `a_low_risk_unit_skips_the_adversary_and_extra_lens` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25741-25811` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25814-25829` `without_a_depth_policy_a_grounded_unit_runs_the_full_panel_and_logs_no_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25832-25910` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25913-26022` `a_flapped_unit_escalates_from_light_at_attempt_0_to_full_on_remediation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26025-26089` `a_stage_level_tiers_policy_routes_the_unit_by_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26092-26174` `every_spawn_runs_in_a_worktree_never_the_main_repo_checkout` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26178-26201` `spec_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26204-26296` `speculation_width_2_runs_two_candidates_first_green_wins_rest_cancelled` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26299-26335` `speculation_budget_accounts_for_every_candidate_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26338-26379` `speculation_defaults_off_runs_a_single_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26382-26422` `speculation_parks_all_candidates_together_in_one_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26425-26547` `speculation_defers_green_until_the_winner_and_integrates_across_replay_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26553-26563` `unit_has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26567-26580` `branch_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26583-26657` `speculation_later_candidate_wins_when_an_earlier_lane_is_review_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26660-26778` `speculation_low_risk_later_lane_winner_routes_light_not_falsely_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26781-26837` `speculation_rejected_loser_is_visible_to_the_review_quality_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26840-26907` `speculation_escalates_when_every_candidate_is_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26910-26975` `speculation_crashed_lane_is_absent_and_a_sibling_still_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26978-27045` `speculation_exhaustive_integrate_door_red_blocks_a_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27048-27155` `speculation_stays_fresh_across_parking_until_a_winner_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27170-27237` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27241-27407` `speculation_blocked_winner_captures_evidence_and_a_later_lane_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27410-27464` `assert_isolated_cwd_refuses_empty_or_repo_root_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27467-27558` `the_conductor_threads_each_agents_persona_to_the_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27561-27615` `the_system_prompt_carries_the_rigger_communication_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27618-27662` `an_agent_with_no_persona_threads_an_empty_system_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27665-27742` `operator_instructions_reach_every_spawned_agent_between_persona_and_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27745-27807` `planner_proposed_unit_inherits_the_default_review_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27810-27905` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27908-27990` `plan_stage_commit_under_specs_reaches_the_run_branch_before_the_next_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27993-28068` `plan_stage_commit_outside_specs_fails_the_stage_naming_the_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28071-28155` `plan_stage_commit_reverting_its_own_out_of_scope_touch_still_fails_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28158-28229` `plan_stage_commit_conflicting_with_a_concurrent_specs_change_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28232-28365` `integrate_plan_commits_is_idempotent_on_a_resumed_already_landed_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28368-28429` `integrate_plan_commits_tolerates_a_pre_existing_intent_record_with_no_git_mutation_yet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28432-28518` `integrate_plan_commits_keeps_the_earlier_commits_identity_when_the_worktree_grows_between_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28531-28547` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28552-28610` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28613-28695` `a_plan_landing_infra_fault_halts_the_run_loudly_with_no_per_unit_lesson_or_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28698-28770` `per_unit_adjudicator_reject_blocks_integration_and_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28773-28866` `an_always_rejecting_adjudicator_escalates_after_exactly_max_retries_cycles` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28869-28966` `a_higher_max_retries_gives_more_attempts_before_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28883-28929` `escalation_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28969-29033` `an_absent_max_retries_preserves_the_default_bound_of_three` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29036-29140` `a_resumed_unit_gets_exactly_its_granted_extra_attempts_before_re_escalating` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29143-29184` `max_retries_for_widens_only_the_resumed_unit_never_a_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29187-29234` `a_stages_own_max_retries_overrides_the_run_default_for_its_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29252-29258` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29261-29294` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29298-29371` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29374-29464` `a_model_ladder_implementer_escalates_one_rung_per_remediation_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29467-29533` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29536-29570` `mid_spawn_crash_escalates_without_aborting_the_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29573-29624` `a_newly_escalated_unit_stamps_an_attention_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29627-29677` `a_budget_halt_stamps_an_attention_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29680-29769` `a_budget_halt_does_not_restamp_on_a_later_poll_with_nothing_new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29772-29876` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29879-29950` `compute_attention_leaves_the_hung_liveness_halt_to_the_caller` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29953-30034` `an_escalation_does_not_restamp_attention_on_a_resumed_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30037-30070` `a_clean_step_stamps_no_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30073-30197` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30200-30247` `nine_of_ten_budget_crosses_the_final_tenth` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30250-30337` `a_re_step_already_past_the_budget_threshold_does_not_re_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30340-30391` `budget_breaker_stops_the_run_after_the_first_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30394-30437` `budget_exhaustion_aborts_the_task` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30440-30478` `a_budget_halt_surfaces_its_reason_on_the_run_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30481-30513` `a_run_within_budget_surfaces_no_halt_reason` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30516-30586` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30589-30631` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30634-30704` `a_review_tier_budget_refusal_aborts_with_budgetexhausted_not_a_raw_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30707-30739` `coverage_gap_flags_a_spec_defect_and_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30754-30763` `run_ungated_fanout_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30766-30794` `ungated_fanout_unit_under_a_gated_template_fails_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30797-30820` `ungated_fanout_unit_under_an_ungated_template_is_not_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30823-30844` `gated_fanout_unit_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30847-30871` `non_fanout_stage_with_no_gates_is_never_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30874-30909` `unmatched_fanout_proposal_under_a_gated_template_fails_the_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30912-30936` `unmatched_fanout_proposal_under_an_ungated_template_is_not_flagged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30939-30956` `ungated_fan_out_templates_names_a_gateless_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30959-30975` `ungated_fan_out_templates_is_silent_on_a_gated_template` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30978-31019` `planner_covering_every_criterion_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31022-31056` `planner_leaving_a_gap_flags_a_spec_defect` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31059-31093` `gate_only_stage_is_a_coverage_proxy_gap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31096-31155` `manual_stage_pauses_while_an_auto_stage_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31158-31201` `isolation_none_agent_gets_no_worktree_even_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31204-31237` `spawn_opts_isolation_is_set_for_a_worktree_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31240-31279` `a_spawned_implementers_title_is_the_unit_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31286-31334` `the_adversarys_spawn_is_stamped_with_the_units_lens_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31340-31389` `the_adjudicators_spawn_is_stamped_with_lenses_plus_adversary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31394-31441` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31450-31513` `the_roster_reflects_the_actually_routed_light_panel_not_the_full_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31527-31584` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31587-31658` `stage_autonomy_override_seeds_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31661-31708` `on_pass_none_runs_gates_but_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31719-31734` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31738-31825` `two_units_gate_environments_never_share_a_target_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31828-31897` `two_units_gate_environments_never_share_a_mutants_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31900-31951` `an_implement_stage_gate_round_creates_no_mutants_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31960-31964` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31967-31976` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31984-32111` `one_build_environment_authority_reaches_both_a_gate_build_and_an_agent_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31999-32036` `run_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32114-32158` `spawn_env_adds_the_per_unit_cargo_target_dir_only_for_a_real_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32161-32224` `a_units_per_unit_cache_is_reclaimed_when_its_worktree_is_removed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32227-32299` `a_units_worktree_cache_and_branch_are_all_reclaimed_on_a_successful_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32302-32362` `a_units_worktree_is_reclaimed_but_its_branch_survives_a_terminal_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32365-32471` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32474-32566` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32569-32671` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32674-32772` `verified_worktree_sha_is_stamped_after_a_gate_side_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32775-32853` `review_tier_boundary_restores_a_worktree_a_prior_tier_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32856-32970` `reviewed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32973-33040` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33043-33157` `a_review_rounds_dirty_residue_is_restored_named_and_never_merged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33160-33241` `a_review_rounds_moved_tip_is_restored_to_the_reviewed_sha_before_merging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33244-33384` `a_review_rounds_lens_residue_survives_a_later_tiers_genuine_crash_and_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33387-33585` `a_review_rounds_log_derived_start_sha_survives_a_cross_call_resume_after_a_later_tiers_park` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33588-33776` `a_review_rounds_log_derived_start_sha_survives_a_same_chunk_sibling_park_across_a_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33779-33955` `a_speculation_lanes_log_derived_start_sha_survives_a_cross_call_resume_after_a_park` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33958-34061` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34064-34149` `a_resumed_reviewed_unit_whose_exhaustive_gate_fails_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34152-34290` `a_resumed_reviewed_units_merge_break_records_an_integrate_conflict_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34293-34438` `a_resumed_landed_but_ungated_unit_regates_the_landed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34441-34622` `a_resumed_reviewed_units_genuine_unresolved_conflict_reaches_the_idempotent_merge_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34639-34685` `regenerate_conflicted_paths_returns_the_real_regeneration_commit_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34688-34762` `a_live_approved_unit_restores_a_worktree_the_integrate_door_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34765-34882` `speculation_lenses_restore_a_gate_deleted_worktree_without_racing_and_stamp_the_winner_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34885-34956` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34959-35028` `speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35031-35151` `a_parked_lens_keeps_the_unit_worktree_beside_a_genuine_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35154-35199` `a_worktree_less_stage_gate_inherits_the_shared_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35202-35239` `bounded_pool_completes_every_stage_under_the_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35242-35351` `run_wave_admits_at_most_max_parallel_units_leaving_the_rest_neither_failed_nor_terminal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35354-35443` `occupancy_survives_a_crash_resume_so_a_still_parked_unit_keeps_its_slot_over_a_fresh_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35446-35492` `live_run_folds_no_gate_machinery` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35495-35538` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35541-35582` `standalone_review_stage_still_takes_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35585-35697` `run_gates_derives_and_injects_the_review_worktrees_store_fence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35700-35753` `a_parked_lens_keeps_the_standalone_review_stages_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35756-35852` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35855-35948` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35951-36036` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36039-36126` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36129-36196` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36199-36226` `partition_separates_overlapping_blast_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36229-36251` `blast_radius_conflicts_flags_units_that_split_one_file_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36254-36267` `blast_radius_conflicts_is_empty_for_a_disjoint_partition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36270-36318` `partitioned_wave_still_integrates_every_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36330-36354` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36358-36467` `lens_finding_reaches_later_tiers_through_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36470-36531` `review_agents_emit_findings_via_the_review_protocol` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36534-36579` `unparseable_adjudicator_output_blocks_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36582-36643` `failing_gate_evidence_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36646-36729` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36732-36802` `a_product_gate_failure_is_not_rerun_and_demotes_as_before` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36805-36883` `an_authored_infra_rule_with_limit_zero_at_an_inline_gate_holds_the_ratchet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36886-36981` `an_inline_gate_red_across_every_rerun_holds_for_infra_and_demotes_for_flaky` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:36984-37023` `unit_evidence_is_populated_after_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37026-37101` `adjudicator_rejection_reasoning_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37104-37171` `fan_out_lens_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37174-37226` `review_only_stage_records_no_artifact_truthfully` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37229-37287` `two_erroring_stages_both_leave_a_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37290-37354` `single_wide_wave_overruns_budget_and_is_stopped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37357-37403` `validate_acyclic_detects_a_cycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37465-37478` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37480-37486` `with_side_effect` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37487-37489` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37490-37492` `targets` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37493-37495` `mutants_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37496-37498` `store_fences` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37499-37501` `build_cache_guards` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37502-37504` `build_cache_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37507-37557` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37570-37607` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37618-37623` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37624-37626` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37629-37649` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37653-37662` `gate_verdict_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37664-37668` `verdict_passed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37671-37737` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37740-37782` `a_content_change_misses_the_cache_and_re_runs_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37785-37841` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37850-37862` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37874-37889` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37890-37892` `blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37893-37895` `index_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37899-37918` `blast_radius_audits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37921-37930` `review_tier_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37935-37958` `tiered_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37960-37967` `tiered_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:37977-38050` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38057-38097` `the_empty_radius_fail_safe_still_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38105-38139` `a_non_structural_grounder_records_no_blast_radius_audit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38151-38216` `confidence_tier_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38227-38278` `confidence_tier_radius_splits_the_one_edge_set_into_extracted_precise_and_all_tier_safe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38288-38365` `grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grep_superset` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38388-38481` `under_symbols_the_audit_emits_and_records_the_prompt_seed_as_precise` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38493-38567` `partition_wave_own_batches_an_empty_radius_never_co_scheduling_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38576-38640` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38643-38694` `staleness_flags_only_downstream_units_whose_radius_intersects_the_touched_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38697-38749` `staleness_grounds_on_the_safe_superset_so_a_grep_only_reference_still_stales_a_downstream_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38752-38850` `rule6_conflict_detection_grounds_on_the_safe_superset_so_a_grep_only_shared_reference_conflicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38853-38883` `a_resume_reseeds_the_stale_set_from_the_prior_unitintegrated_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38892-38933` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:38937-39062` `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39065-39091` `glob_matches_supports_star_doublestar_and_literals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39094-39118` `gate_intersects_radius_runs_unscoped_and_intersecting_gates_only` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39121-39270` `blast_radius_narrows_the_inner_loop_and_the_integrate_step_runs_the_full_library` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39273-39401` `a_gate_skipped_inline_but_red_at_the_exhaustive_integrate_door_blocks_the_merge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39404-39433` `verdict_compensates_names_a_prior_unit_independent_of_approval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39446-39488` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39492-39667` `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39670-39721` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39724-39783` `commits_to_compensate_dedupes_a_repeated_sha_and_skips_an_already_compensated_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39786-39837` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39840-39900` `pending_compensations_from_log_re_derives_only_undrained_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39903-39934` `a_durable_compensation_queued_mark_is_fold_neutral_on_the_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39942-39979` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:39987-40058` `assert_compensated_unit_re_gates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40087-40132` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40172-40196` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40200-40335` `a_resume_re_drives_a_durably_queued_but_undrained_compensation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40355-40403` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40407-40589` `integrate_re_gates_the_merged_tree_and_a_merge_break_blocks_the_second_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40612-40679` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40683-40853` `the_post_merge_re_gate_runs_in_its_own_scratch_worktree_never_the_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40870-40886` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40891-40967` `postmerge_run_gates_err_still_reaps_the_throwaway_worktree_and_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:40970-41049` `postmerge_worktree_create_err_still_reaps_the_just_created_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41052-41172` `conflict_regenerate_pending_from_log_re_derives_the_union_keyed_by_unit_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41180-41206` `conflict_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41209-41295` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41298-41378` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41381-41485` `integrate_conflict_exhausted_after_the_bound_charges_a_real_attempt_with_the_unresolved_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41488-41646` `integrate_conflict_records_regenerate_pending_before_the_accept_incoming_mutation_that_can_fail` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41528-41571` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41649-41723` `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41726-41790` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41801-41824` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41828-41899` `a_default_infra_fault_at_a_deferred_gate_does_not_demote` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:41902-42006` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42009-42094` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42097-42241` `a_parked_step_never_records_a_partial_tree_deferred_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42244-42328` `a_manual_review_paused_unit_defers_the_whole_tree_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42331-42464` `an_escalated_dep_still_runs_the_whole_tree_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42467-42581` `a_recorded_failing_deferred_verdict_re_surfaces_its_failure_on_replay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42584-42670` `a_replayed_fan_out_review_reject_appends_no_duplicate_unitfailed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42673-42721` `a_fan_out_review_stage_whose_gates_fail_after_approval_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42727-42741` `run_git` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42745-42753` `git_head` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42764-42795` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42799-42858` `gate_measures_the_committed_artifact_not_the_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42861-42987` `conductor_spawns_the_sdet_author_at_the_build_seam_so_its_tests_land_in_the_committed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:42990-43080` `the_sdet_author_spawn_respects_the_budget_breaker_at_its_own_spawn_site` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43083-43185` `a_parked_sdet_author_spawn_holds_the_unit_instead_of_integrating_without_its_periphery_tests` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43188-43300` `speculation_sdet_author_periphery_lands_in_the_committed_tree_the_gates_judge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43303-43400` `a_parked_sdet_author_in_a_speculation_candidate_holds_the_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43403-43478` `an_empty_accounting_sdet_author_result_advances_the_build_to_commit_gates_and_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43481-43545` `an_absent_sdet_author_is_a_clean_no_op_and_the_build_still_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43548-43623` `a_crashed_sdet_author_spawn_does_not_block_the_build_the_lifecycle_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43626-43726` `an_escalated_unit_does_not_integrate_its_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43734-43744` `critique_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43773-43784` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43790-43796` `rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43799-43849` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43853-43922` `a_blast_radius_overlap_alone_does_not_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43935-43979` `the_plan_critique_gates_adversary_and_adjudicator_spawns_are_stamped_correctly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:43982-44019` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44022-44073` `a_clean_decomposition_approves_and_releases_the_fan_out` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44076-44151` `the_gate_critiques_only_not_yet_run_units_and_approves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44154-44191` `the_plan_critique_prompt_names_the_cross_unit_rules` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44194-44219` `the_critique_gate_never_enters_a_wave_even_when_ready` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44228-44262` `fan_out_needs_template_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44265-44347` `a_downstream_stage_needing_the_fan_out_template_stays_unready_until_every_member_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44350-44454` `a_real_split_pair_must_both_integrate_not_just_the_btreemap_key_first_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44457-44531` `a_resolved_plan_critique_gate_does_not_re_run_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44534-44632` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44635-44691` `an_approved_gate_releases_planner_proposed_units_not_only_baselines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44694-44764` `the_re_plan_directive_instructs_reusing_the_existing_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44777-44782` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44785-44805` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44809-44880` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44883-44937` `a_parked_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:44940-44990` `a_budget_refused_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:45003-45030` `the_conductors_one_event_authority_reports_a_write_the_store_lost` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `conductor::tests::support` (7 functions)
  - `src/conductor.rs:14036-14038` `occurrences` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14041-14043` `snapshot` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14047-14061` `stage_shape` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14074-14077` `apply` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14078-14081` `apply_batch` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14082-14084` `subgraph` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14085-14087` `resolve` - defined inside `support`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::dash_marker` (4 functions)
  - `src/dash.rs:518-520` `serialize` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:526-531` `parse` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:535-537` `read` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
  - `src/dash.rs:542-544` `write` - method inside `impl DashMarker`; grouped with its other `DashMarker` methods.
- `dash::reaped_child` (4 functions)
  - `src/dash.rs:3779-3781` `new` - method inside `impl ReapedChild`; grouped with its other `ReapedChild` methods.
  - `src/dash.rs:3785-3787` `id` - method inside `impl ReapedChild`; grouped with its other `ReapedChild` methods.
  - `src/dash.rs:3796-3798` `child_mut` - method inside `impl ReapedChild`; grouped with its other `ReapedChild` methods.
  - `src/dash.rs:3802-3813` `drop` - method inside `impl Drop for ReapedChild`; grouped with its other `ReapedChild` methods.
- `dash::registry` (6 functions)
  - `src/dash.rs:335-345` `dash_serving_pid_on` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:512-514` `displayable_pid` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:586-608` `pid_holding_port` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:752-754` `pid_if_port_matches` - name contains "pid" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:972-993` `instance_views` - name contains "instance" (instance registry); grouped under `dash::registry`.
  - `src/dash.rs:998-1000` `instances_json` - name contains "instance" (instance registry); grouped under `dash::registry`.
- `dash::render` (18 functions)
  - `src/dash.rs:351-381` `header_line_value` - name contains "header" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:387-410` `head_has_header_line` - name contains "header" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:637-652` `format_held_port` - name contains "format" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:668-670` `describe_held_port` - name contains "describe" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:717-719` `describe_held_port_if_confirmed` - name contains "describe" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1486-1518` `build_graph_view` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1785-1799` `rationale_batch` - name contains "rationale" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1815-1847` `graph_json` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1885-1897` `call_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:1914-1926` `ref_node_view` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2217-2298` `unit_node` - name contains "node" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2302-2361` `role_stage` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2409-2420` `stage_of_role` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2428-2449` `role_and_agent` - name contains "role" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2530-2538` `graph_seeds` - name contains "graph" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2729-2735` `field_str` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2739-2750` `field_str_array` - name contains "field" (HTML rendering); grouped under `dash::render`.
  - `src/dash.rs:2928-2941` `escape_for_script` - name contains "escape" (HTML rendering); grouped under `dash::render`.
- `dash::reproject` (5 functions)
  - `src/dash.rs:1554-1567` `reproject` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1575-1594` `cap_clusters` - name contains "cluster" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1612-1623` `reprojection_lens_key` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1629-1672` `reproject_derived` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
  - `src/dash.rs:1693-1778` `reproject_files` - name contains "reproject" (graph reprojection); grouped under `dash::reproject`.
- `dash::response` (5 functions)
  - `src/dash.rs:2964-2970` `rendered` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:2971-2977` `text` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:2983-2989` `binary` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:2991-3001` `reason` - method inside `impl Response`; grouped with its other `Response` methods.
  - `src/dash.rs:3011-3027` `write_to` - method inside `impl Response`; grouped with its other `Response` methods.
- `dash::server` (8 functions)
  - `src/dash.rs:440-452` `bind_singleton` - name contains "bind" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:556-575` `tcp_listen_inode_for_port` - name contains "tcp" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:616-624` `process_state` - name contains "process_" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:2042-2079` `calls_route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3036-3217` `route` - name contains "route" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3293-3335` `serve_on` - name contains "serve" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3351-3530` `handle_conn` - name contains "handle" (HTTP serving); grouped under `dash::server`.
  - `src/dash.rs:3619-3744` `serve_console_stream` - name contains "serve" (HTTP serving); grouped under `dash::server`.
- `dash::tests` (105 functions)
  - `src/dash.rs:3935-3947` `seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3949-3958` `local_instance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:3966-4016` `instance_views_project_a_sorted_credential_free_landing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4023-4031` `instance_view_age_floors_at_zero_for_a_future_heartbeat` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4038-4070` `api_instances_route_renders_the_landing_list` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4077-4093` `console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4098-4110` `console_core_wasm_artifact_is_under_the_three_megabyte_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4122-4150` `embedded_artifact_matches_a_fresh_independent_nested_build` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4162-4223` `console_route_serves_the_shell_page_with_mock_regions_and_both_theme_token_blocks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4229-4251` `console_route_never_references_an_external_url` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4258-4316` `console_fonts_route_serves_each_embedded_font_and_its_license_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4321-4335` `console_fonts_route_404s_for_an_unknown_asset` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4343-4369` `console_page_wires_the_theme_toggles_persistence_round_trip` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4372-4397` `root_serves_the_embedded_page_with_the_placeholder_resolved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4408-4458` `gates_status_never_fabricates_passed_for_an_off_linear_unverdicted_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4465-4501` `free_port_from_returns_the_start_port_when_free_and_the_next_free_one_when_it_is_taken` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4508-4557` `bind_singleton_binds_the_exact_port_and_never_searches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4568-4641` `bind_singleton_short_circuits_on_an_already_serving_rigger_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4648-4664` `dash_serving_on_is_false_for_a_non_dash_listener` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4675-4720` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4743-4794` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4805-4825` `dash_serving_pid_on_reports_the_pid_a_real_dash_response_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4847-4870` `dash_serving_pid_on_assembles_a_head_that_spans_many_read_calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4877-4892` `dash_serving_pid_on_is_none_for_a_non_dash_listener` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4897-4900` `dash_serving_pid_on_is_none_when_nothing_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4920-4940` `dash_serving_pid_on_is_none_when_the_pid_header_value_is_not_a_number` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4948-4977` `bind_singleton_cold_race_loser_resolves_across_the_accept_window` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:4988-5023` `the_page_layout_cannot_scroll_the_body_horizontally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5034-5071` `the_landing_view_lists_instances_and_threads_the_attach_selector` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5076-5085` `css_rule` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5096-5141` `the_dashboard_fits_one_screen_with_internal_scroll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5155-5189` `cells_fit_or_wrap_and_wide_cells_scroll_in_their_own_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5204-5206` `the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5209-5237` `state_endpoint_projects_the_seeded_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5244-5284` `console_event_filter_admits_only_the_named_run_lifecycle_types` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5290-5298` `console_event_wire_matches_console_cores_wire_event_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5306-5312` `console_event_wire_carries_recorded_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5318-5325` `console_event_wire_with_a_malformed_body_degrades_to_null_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5331-5338` `console_progress_wire_with_a_malformed_body_degrades_to_empty_id_and_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5341-5422` `console_snapshot_endpoint_filters_events_carries_progress_liveness_and_definitions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5425-5486` `state_carries_the_live_agent_activity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5489-5555` `state_counts_grep_fallbacks_and_carries_them_in_the_review_outcomes_data` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5564-5750` `run_tree_projects_the_spine_with_collapse_expand_and_driver_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5570-5575` `done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5755-5784` `review_verdicts_come_straight_from_the_metrics_classification` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5787-5797` `events_endpoint_is_since_exclusive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5803-5832` `no_mutating_endpoint_exists` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5835-5849` `unknown_get_path_is_404` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5852-5881` `export_inlines_the_snapshot_as_a_static_page` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5889-5936` `export_neutralizes_a_script_breakout_in_the_inlined_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5939-5965` `decision_view_strikes_through_superseded_entries` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:5974-6037` `cluster_key_folds_paths_by_directory_and_dev_loop_nodes_by_kind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6049-6142` `clustered_overview_under_files_lens_admits_only_code_entities_keyed_by_their_own_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6161-6404` `cluster_detail_drills_a_cluster_to_its_members_and_caps_a_big_one_by_degree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6413-6458` `cluster_detail_under_files_lens_is_unconditionally_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6475-6672` `code_lens_buckets_code_entities_by_community_excludes_other_kinds_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6686-6925` `concepts_lens_buckets_members_by_concept_excludes_membershipless_nodes_and_reports_underived_grain` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6931-6948` `tiered_chain_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:6951-7015` `the_graph_route_returns_a_tier_tagged_seeded_neighborhood_as_json` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7018-7069` `the_graph_route_percent_decodes_the_seed_so_select_to_seed_reaches_ids_with_special_chars` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7072-7103` `the_graph_route_degrades_gracefully_for_an_unknown_seed_and_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7106-7126` `the_graph_route_is_read_only_a_non_get_is_405` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7132-7166` `dispatch_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7175-7273` `the_graph_route_dispatches_cluster_overview_and_seed_by_parameter` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7287-7341` `the_overview_route_degrades_gracefully_on_an_empty_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7344-7398` `neighborhood_bounds_by_depth_follows_both_directions_and_skips_invalidated_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7401-7434` `neighborhood_flags_god_nodes_by_degree_within_the_returned_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7437-7509` `path_is_the_shortest_route_between_two_selected_nodes_over_currently_valid_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7512-7579` `the_graph_route_flags_god_nodes_and_returns_the_query_path_between_two_selected_nodes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7586-7623` `provenance_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7626-7673` `explain_returns_a_nodes_incident_edges_as_source_and_tier_tagged_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7676-7741` `the_graph_route_carries_the_seed_nodes_explain_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7744-7775` `graph_seeds_enumerate_decisions_findings_and_their_files_never_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7778-7822` `a_units_seed_lands_on_the_neighborhood_of_its_decisions_and_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7825-7931` `the_run_tree_click_to_seed_route_lands_a_unit_on_a_real_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7934-7987` `unit_seeds_scope_content_to_the_owning_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:7990-8026` `repoint_seed_passes_a_known_node_and_re_points_a_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8029-8048` `build_state_on_an_empty_run_is_empty_not_a_panic` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8059-8148` `release_ready_is_surfaced_on_the_dash_only_for_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8166-8221` `release_ready_pr_command_newline_renders_as_a_real_line_break_not_a_collapsed_run_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8224-8231` `request_line_parsing_extracts_method_and_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8234-8241` `query_param_reads_since` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8249-8327` `endpoints_serve_over_a_real_socket_against_a_seeded_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8333-8384` `a_post_over_a_real_socket_is_refused_without_touching_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8395-8507` `the_graph_provider_is_consulted_only_on_graph_requests_not_the_state_poll` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8512-8522` `dash_marker_round_trips_through_its_on_disk_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8525-8544` `dash_marker_parse_rejects_a_malformed_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8547-8565` `dash_marker_reads_none_for_an_absent_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8570-8582` `format_held_port_always_names_the_address_even_with_no_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8585-8602` `format_held_port_names_the_pid_and_state_for_a_running_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8605-8613` `format_held_port_names_the_pid_alone_when_its_state_is_not_discoverable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8616-8631` `format_held_port_gives_the_stopped_listener_diagnosis_naming_resume_or_kill` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8634-8646` `pid_holding_port_finds_the_pid_of_a_listener_bound_in_this_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8649-8665` `pid_holding_port_is_none_for_a_port_nothing_is_listening_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8668-8686` `describe_held_port_names_this_process_when_it_holds_the_port_itself` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8701-8717` `describe_held_port_if_confirmed_names_the_holder_when_independently_confirmed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8724-8740` `describe_held_port_if_confirmed_is_none_when_nothing_holds_the_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8743-8760` `dash_start_needed_is_true_when_none_serving_and_false_when_one_serves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8763-8811` `dash_status_trusts_a_url_with_no_marker_and_catches_a_marker_that_lies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8823-8839` `dash_status_never_names_the_unattributed_pid_sentinel_as_a_dead_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8842-8863` `url_port_parses_the_recorded_shape_and_rejects_anything_else` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8879-8919` `dash_status_probes_the_urls_own_port_when_the_marker_names_a_different_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8922-8960` `should_reap_singleton_reaps_only_when_no_registered_instance_is_live` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:8963-8996` `should_reap_singleton_never_reaps_while_a_fresh_agent_liveness_signal_is_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/dash.rs:9008-9079` `the_page_carries_the_directed_call_layered_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
- `dash::tests::calls_route_c4` (10 functions)
  - `src/dash.rs:9097-9107` `cnode` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9109-9111` `layer_of` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9112-9114` `ids` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9120-9175` `calls_view_down_signs_callees_positive_and_carries_frontier_and_back` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9180-9219` `calls_view_up_negates_callers_and_carries_the_referenced_sidecar` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9225-9273` `calls_view_both_centers_the_seed_with_callees_right_and_callers_left` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9279-9316` `a_plain_neighborhood_omits_every_additive_call_field` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9323-9377` `calls_route_runs_the_traversal_for_view_calls_and_declines_otherwise` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9383-9411` `calls_route_clamps_depth_and_defaults_the_tier_floor` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9416-9434` `calls_route_walks_both_directions_for_dir_both` - defined inside `calls_route_c4`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::metadata_card_c2` (13 functions)
  - `src/dash.rs:10028-10078` `card_graph` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10081-10124` `card_of_a_code_entity_carries_file_line_degree_community_concepts_and_memory_counts` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10130-10138` `card_of_a_membership_less_entity_has_no_community_and_no_line` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10146-10173` `card_of_a_proven_code_entity_carries_proven_by_and_proof_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10181-10186` `card_of_an_unproven_code_entity_has_proven_by_zero_and_no_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10192-10205` `card_of_a_file_reports_no_proof_of_its_own` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10212-10222` `card_tolerates_a_malformed_proof_evidence_attr` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10229-10266` `card_of_a_file_lists_its_contained_entities_as_top_entities` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10271-10291` `card_of_a_concept_lists_its_realizing_members_as_top_evidence` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10300-10325` `card_of_a_concept_carries_each_top_evidence_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10332-10346` `card_of_a_file_carries_each_top_entity_members_own_kind` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10351-10354` `card_of_an_unknown_id_is_none` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:10360-10402` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one` - defined inside `metadata_card_c2`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::rationale_overlay_c3` (12 functions)
  - `src/dash.rs:9456-9466` `finding_node` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9482-9515` `rationale_graph` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9518-9520` `leaf_fields` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9526-9540` `node_rationale_returns_attached_leaves_ordered_by_kind_then_id` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9546-9569` `a_finding_leaf_carries_content_only_never_the_by_or_unit_machinery` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9575-9588` `a_handbook_rule_and_a_supersedes_edge_are_not_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9593-9601` `an_invalidated_edge_is_not_live_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9606-9616` `a_node_without_rationale_returns_none` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9622-9647` `the_batch_covers_the_visible_set_and_keeps_only_nodes_with_rationale` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9652-9669` `the_batch_is_deterministic_across_request_order_and_dedups` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9675-9714` `the_explain_route_returns_the_batch_in_one_request` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9720-9744` `an_absent_explain_leaves_the_graph_route_unchanged` - defined inside `rationale_overlay_c3`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::subject_view_c5` (5 functions)
  - `src/dash.rs:9765-9808` `memory_rail_lists_decisions_findings_and_concepts_excluding_lessons` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9813-9820` `memory_rail_is_empty_for_a_node_with_no_governing_memory` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9841-9918` `memory_rail_concepts_are_live_from_node_realizes_edges_to_a_concept_target_deduped_by_id` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9926-9976` `the_seeded_route_carries_memory_without_adding_a_single_node_to_the_neighborhood` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:9982-10002` `a_cluster_drill_carries_no_memory_field` - defined inside `subject_view_c5`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `dash::tests::supervised_lifecycle` (4 functions)
  - `src/dash.rs:3832-3841` `spawn_blocking_child` - defined inside `supervised_lifecycle`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:3846-3855` `watch_for_exit` - defined inside `supervised_lifecycle`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:3858-3874` `reaped_child_reaps_even_when_the_driver_panics` - defined inside `supervised_lifecycle`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
  - `src/dash.rs:3877-3901` `dropping_the_peers_sidecar_reaps_its_collector_thread` - defined inside `supervised_lifecycle`, a #[cfg(test)] module; proposed home groups it with that named test subgroup pending consolidation (spec 85 section 5).
- `main::commands` (41 functions)
  - `src/main.rs:279-282` `cmd_version` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2216-2224` `cmd_run` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2328-2410` `cmd_resume_unit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2412-2975` `cmd_step` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3205-3237` `cmd_reported` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3255-3271` `cmd_prompt` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3295-3326` `cmd_scratch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3964-3971` `cmd_serve` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4016-4085` `cmd_workflow` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4154-4206` `cmd_graph` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4329-4349` `cmd_graph_show` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4653-4736` `cmd_graph_build` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4757-4816` `cmd_graph_communities` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4836-4895` `cmd_graph_concepts` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4914-4952` `cmd_stats` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5276-5286` `cmd_stats_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5667-5790` `cmd_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5800-5836` `cmd_playbooks` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5869-6021` `cmd_replay` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:6857-7167` `cmd_dash` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7628-7664` `cmd_ground` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7680-7701` `cmd_reindex` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7718-7746` `cmd_symbols_index` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7754-7830` `cmd_emit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7841-7875` `cmd_progress` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7937-8169` `cmd_status` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8256-8287` `cmd_watch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8543-8624` `cmd_reset` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9327-9367` `cmd_peers` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9574-9670` `cmd_result` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9806-10006` `cmd_validate` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12119-12138` `cmd_init` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12786-12958` `cmd_setup` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13336-13341` `cmd_docs` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13496-13500` `cmd_instructions` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13508-13546` `cmd_prime` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13666-13714` `cmd_mcp` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:13969-14006` `cmd_grep_guard` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:14271-14303` `cmd_guard_write` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:14308-14320` `cmd_hook` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:14334-14401` `cmd_hook_stop_failure` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
- `main::dash_glue` (25 functions)
  - `src/main.rs:120-124` `record_dash_attempt` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6367-6369` `dash_marker_serving` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6449-6469` `spawn_dash_child_process` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6494-6515` `wait_for_dash_bind` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6542-6575` `wait_for_dash_bind_or_diagnose` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6728-6730` `dash_ensure_suppressed` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6737-6739` `dash_ensure_port` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6745-6748` `dash_ensure_port_from` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6809-6813` `recorded_dash_url` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6823-6837` `dash_status_line` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6846-6855` `dash_status_json` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7174-7181` `dash_reap_poll` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7190-7198` `dash_reap_idle_window` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7313-7332` `dash_read_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7338-7350` `dash_read_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7361-7369` `dash_read_whole_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7378-7395` `dash_read_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7401-7413` `dash_attach_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7418-7435` `dash_read_progress` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7441-7468` `dash_read_liveness` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7491-7507` `dash_resolve_attach` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7532-7542` `dash_read_sqlite_stream_readonly` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7544-7580` `dash_attach_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7587-7599` `dash_attach_inputs` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:7605-7611` `dash_attach_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
- `main::docs_overlay` (1 function)
  - `src/main.rs:12272-12279` `apply` - method inside `impl DocsOverlay`; grouped with its other `DocsOverlay` methods.
- `main::liveness` (9 functions)
  - `src/main.rs:3029-3039` `live_branches_for_sweep` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:3070-3086` `terminal_and_no_live_worker` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:7891-7917` `liveness_ages_for_wave` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9023-9061` `live_writer_reasons` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9067-9079` `live_writer_refusal` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9243-9253` `superseded_edge_boundary` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9275-9305` `superseded_graph_nodes` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:10797-10804` `live_slugs` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:11025-11043` `worktree_belongs_to_live` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
- `main::provenance` (22 functions)
  - `src/main.rs:209-211` `workflow_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3552-3594` `refuse_when_base_lacks_spec_paths` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3818-3962` `run_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3977-4008` `parse_workflow_args` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9799-9804` `spec_lint_warning_lines` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10255-10260` `installed_workflow_drifted` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10267-10269` `workflow_provenance_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10274-10282` `installed_workflow_provenance` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:10301-10312` `git_is_ancestor` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:10320-10330` `git_commit_distance` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:10457-10491` `workflow_drift_advisory` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12207-12217` `install_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12287-12296` `read_docs_overlay` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12621-12642` `git_hooks_dir` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:13256-13300` `docs_context` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13356-13375` `docs_drift` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13384-13400` `docs_drift_failure` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:13422-13428` `spec_lint_next_step` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13449-13453` `spec_lint_reminder_suppressed` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13461-13473` `spec_lint_reminder_should_print` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:13587-13590` `git_repo` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:13598-13608` `git_repo_at` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
- `main::render` (10 functions)
  - `src/main.rs:4238-4305` `print_around_subgraph` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4359-4397` `print_entity_site` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4406-4418` `print_site_header` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5049-5091` `format_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5137-5153` `format_progress_line` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5315-5411` `format_canary_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:6217-6227` `format_stats_diff` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:11258-11288` `format_residue` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:11958-11970` `print_orientation` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:14403-14422` `print_run_state` - name contains "print_" (human-readable output); grouped under `main::render`.
- `main::replay_runner` (1 function)
  - `src/main.rs:6238-6258` `run` - method inside `impl Runner for ReplayRunner`; grouped with its other `ReplayRunner` methods.
- `main::residue_report` (1 function)
  - `src/main.rs:10568-10573` `is_empty` - method inside `impl ResidueReport`; grouped with its other `ResidueReport` methods.
- `main::run_registration` (2 functions)
  - `src/main.rs:769-774` `inert` - method inside `impl RunRegistration`; grouped with its other `RunRegistration` methods.
  - `src/main.rs:778-785` `drop` - method inside `impl Drop for RunRegistration`; grouped with its other `RunRegistration` methods.
- `main::scaffold_report` (1 function)
  - `src/main.rs:11798-11805` `changed` - method inside `impl ScaffoldReport`; grouped with its other `ScaffoldReport` methods.
- `main::setup` (20 functions)
  - `src/main.rs:2018-2021` `scratch_defaults` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:4100-4118` `locate_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:7217-7221` `foreign_instance_scratch_root` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11359-11379` `scratch_totals` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11541-11576` `classify_agent_scratch` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11942-11949` `print_scaffold_pointer` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:12083-12117` `scaffold_summary_lines` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:12176-12193` `install_file_if_changed` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12225-12227` `skill_source_rel` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/main.rs:12236-12241` `skill_install_path` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12309-12322` `install_skills` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12654-12682` `install_precommit_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12699-12707` `provision_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12721-12730` `shim_is_current` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12735-12742` `write_shim_files` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:12749-12777` `run_npm_install` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:12983-12999` `install_lookup_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:13017-13033` `install_status_line` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:13042-13056` `install_operator_mcp` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:13068-13084` `parse_setup_args` - name contains "setup" (project setup); grouped under `main::setup`.
- `main::store` (33 functions)
  - `src/main.rs:482-500` `store_conn_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:557-570` `store_backend_kind` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:620-686` `store_selection_at` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:694-700` `store_selection` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:728-741` `registry_store_identity` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1253-1324` `migrate_project_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1333-1338` `migrate_local_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1350-1376` `migrate_identity_at` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1785-1787` `find_store_dir_from` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2032-2037` `server_store_location` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2068-2133` `require_store_dir` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2137-2139` `store_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:3132-3138` `reclaim_run_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8643-8669` `reset_menu` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8738-8778` `reset_modes` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8805-8824` `reset_scratch_orphans` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8826-8845` `reset_build_cache` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8850-8863` `build_cache_reclaim_report` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8891-8899` `reset_derived` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9110-9171` `refuse_derived_reset_if_live` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9198-9229` `reset_runs` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9703-9718` `reclaim_spawn_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9745-9762` `reclaim_spawn_registered_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10178-10188` `bloat_advisory` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10201-10212` `bloat_advisory_for` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10832-10921` `reclaim_orphan_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11196-11237` `reclaim_shared_build_cache` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11396-11439` `scratch_footprint` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11449-11472` `store_and_backup_bytes` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11593-11670` `footprint_report` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11676-11681` `footprint_report_lines` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11690-11710` `footprint_advisories` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:11741-11768` `footprint_report_for` - name contains "footprint" (store hygiene); grouped under `main::store`.
- `main::store_location` (3 functions)
  - `src/main.rs:1961-1963` `file` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1969-1976` `identity` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1989-1995` `repo_root` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
- `main::store_selection` (1 function)
  - `src/main.rs:443-445` `is_sqlite` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
- `main::support` (36 functions)
  - `src/main.rs:330-406` `parse_run_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:417-425` `resolve_run_base` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:507-509` `conn_file_is_group_or_other_readable` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:517-539` `warn_if_conn_file_is_exposed` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:577-600` `resolve_conn` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:708-718` `resolve_store` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:988-997` `read_project_id` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:1833-1856` `resolve_main_worktree_or_refuse` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:3384-3427` `parse_step_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:5421-5433` `read_model_drift` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5442-5455` `read_graph_index_lag` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5541-5553` `read_order_signatures` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:5577-5649` `parse_canary_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:6098-6130` `parse_replay_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:6382-6408` `ensure_run_dashboard_at` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:6768-6803` `ensure_run_dashboard` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:8199-8229` `parse_watch_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:9438-9496` `parse_result_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:9536-9550` `read_outcome_from_stdin` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:10673-10701` `read_run_units` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:11047-11049` `is_uuid8` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:11056-11085` `find_shadow_stores` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:11148-11153` `is_build_cache_tombstone` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:11989-12038` `write_gitignore_entries` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:12523-12528` `find_bytes` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:13310-13328` `write_docs` - name contains "write_" (generic write helper); grouped under `main::support`.
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
- `main::tests` (383 functions)
  - `src/main.rs:12610-12615` `compose_precommit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14665-14679` `test_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14688-14695` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14703-14705` `spec_lint_reminder_suppressed_when_env_names_the_real_direct_parent_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14711-14732` `spec_lint_reminder_prints_on_absent_foreign_stale_or_malformed_env` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14749-14771` `spec_lint_warning_lines_formats_every_advisory_with_the_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14776-14779` `spec_lint_warning_lines_is_empty_on_a_clean_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14789-14842` `ensure_run_dashboard_at_starts_once_then_short_circuits_on_a_live_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14851-14857` `dash_marker_serving_reports_false_when_nothing_answers_the_markers_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14862-14894` `ensure_run_dashboard_at_restarts_when_the_recorded_dash_is_gone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14902-14921` `ensure_run_dashboard_at_reports_failed_when_the_start_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14933-14972` `wait_for_dash_bind_confirms_promptly_once_the_port_answers_as_a_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14979-15006` `wait_for_dash_bind_fails_fast_when_the_child_exits_before_confirming` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15020-15062` `wait_for_dash_bind_times_out_against_a_real_held_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15085-15129` `wait_for_dash_bind_or_diagnose_never_self_attributes_its_own_still_starting_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15146-15174` `ensure_run_dashboard_at_writes_no_marker_when_the_real_spawn_never_confirms_a_bind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15189-15234` `ensure_run_dashboard_at_names_the_held_ports_holder_when_the_real_spawn_fails` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15254-15278` `ensure_run_dashboard_at_never_claims_a_phantom_holder_for_an_unheld_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15290-15321` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_dead_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15331-15361` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_live_pid_whose_port_is_unserved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15374-15412` `ensure_run_dashboard_at_never_revisits_a_still_serving_unattributed_pid_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15419-15440` `dash_ensure_is_suppressed_by_either_opt_out_and_proceeds_when_neither_is_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15450-15481` `dash_status_line_renders_each_outcome_to_its_exact_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15489-15517` `dash_status_json_renders_each_outcome_to_its_exact_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15526-15553` `dash_ensure_port_defaults_to_the_fixed_address_and_only_a_valid_override_relocates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15563-15596` `compose_precommit_fresh_install_carries_the_managed_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15606-15633` `precommit_block_refuses_on_drift_instead_of_staging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15647-15700` `precommit_block_finds_the_relocated_unit_target_with_the_binary_s_own_path_encoding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15703-15735` `shipped_workflow_driver_tells_a_worker_its_units_build_location` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15738-15839` `precommit_block_resolves_a_tree_built_binary_before_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15846-15858` `compose_precommit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15869-15898` `compose_precommit_chains_without_clobbering_an_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15908-15922` `compose_precommit_prepends_before_a_terminal_exit_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15931-15982` `install_precommit_hook_preserves_a_non_utf8_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15989-16012` `compose_precommit_refreshes_a_stale_block_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16020-16039` `cmd_peers_prints_live_or_historical_per_decision_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16057-16097` `superseded_graph_nodes_drops_dead_runs_and_preboundary_keeping_lessons_active_and_reused_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16059-16064` `run_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16106-16136` `superseded_edge_boundary_is_the_active_runs_start_or_none_without_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16108-16111` `event_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16164-16418` `the_denoise_leaves_metrics_run_pruning_and_blast_radius_unaffected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16428-16446` `version_line_carries_the_derived_version_and_a_non_empty_build_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16453-16497` `docs_context_reads_every_fact_from_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16504-16545` `docs_render_surfaces_known_code_facts_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16551-16570` `commands_registry_is_well_formed_and_covers_dispatch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16578-16608` `write_docs_writes_every_registry_skill_plus_the_handbook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16618-16655` `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16664-16703` `per_operation_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16713-16741` `watching_discipline_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16749-16816` `docs_drift_flags_a_changed_file_and_skips_absent_or_in_sync_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16827-16853` `committed_registry_docs_are_in_sync_with_a_fresh_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16864-16893` `planning_field_guide_page_renders_and_is_linked_from_authoring_loops` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16904-16973` `status_and_dashboard_render_the_same_current_blocker_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16986-17041` `release_ready_lines_surface_only_on_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17044-17110` `status_and_dash_read_the_runs_persisted_base_not_a_re_resolution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17115-17136` `dirty_tracked_paths_keeps_tracked_modifications_and_drops_untracked_and_ignored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17139-17141` `dirty_tracked_paths_on_a_clean_tree_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17144-17169` `installed_workflow_drifted_is_false_when_absent_or_identical_and_true_on_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17174-17221` `drift_side_names_the_binary_stale_only_when_the_installed_workflow_is_provably_newer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17224-17272` `workflow_drift_advisory_names_which_side_is_stale_and_never_says_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17275-17319` `git_is_ancestor_decides_commit_order_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17322-17368` `git_commit_distance_counts_commits_ahead_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17371-17385` `missing_gitsemver_binary_advisory_fires_only_on_the_unversioned_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17388-17393` `behind_the_tree_message_is_silent_when_versions_already_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17396-17406` `behind_the_tree_message_is_silent_when_either_side_is_unversioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17409-17418` `behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17421-17428` `behind_the_tree_message_names_both_versions_and_the_commit_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17431-17469` `behind_the_tree_advisory_names_the_real_derived_version_ahead_of_the_installed_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17472-17494` `behind_the_tree_advisory_is_silent_when_the_checkout_has_not_moved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17497-17510` `install_workflow_records_the_build_provenance_beside_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17513-17528` `validate_advisories_warns_on_workflow_drift_naming_the_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17534-17536` `slugs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17541-17569` `init_committed_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17587-17603` `resolve_main_worktree_or_refuse_returns_exactly_git_rev_parse_show_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17606-17630` `refuse_when_base_lacks_spec_paths_refuses_on_total_absence_and_names_a_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17633-17650` `refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17653-17671` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17674-17714` `refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17717-17798` `refuse_when_base_unreachable_fails_loudly_only_when_no_reachable_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17801-17808` `human_size_formats_bytes_through_gib` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17811-17817` `is_uuid8_accepts_exactly_eight_hex_digits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17820-17865` `worktree_belongs_to_live_matches_both_naming_shapes_without_prefix_false_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17868-17915` `current_run_units_scopes_to_the_current_run_and_splits_live_from_dead` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17918-17955` `current_run_units_spares_a_terminal_units_branch_whose_latest_spawn_is_still_in_flight` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17958-17984` `current_run_units_still_retires_a_terminal_unit_once_its_latest_spawn_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17987-18034` `current_run_units_splits_live_spawns_from_answered_ones_scoped_to_the_current_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18046-18072` `live_branches_for_sweep_fails_closed_on_an_unreadable_stream_but_folds_a_readable_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18075-18115` `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18118-18129` `dir_size_bytes_sums_files_recursively` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18134-18150` `reclaim_shared_build_cache_deletes_a_populated_cache_and_reports_its_bytes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18153-18171` `reclaim_shared_build_cache_is_idempotent_zero_report_when_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18174-18202` `reclaim_shared_build_cache_refuses_rather_than_waits_when_a_build_holds_the_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18205-18228` `reclaim_shared_build_cache_releases_the_guard_promptly_after_a_successful_reclaim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18231-18304` `scan_residue_reports_dead_worktrees_caches_shadows_and_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18307-18326` `scan_residue_is_empty_when_everything_is_live_and_no_shadow_stores` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18329-18346` `format_residue_renders_a_sized_warning_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18351-18370` `store_and_backup_bytes_splits_live_store_files_from_dot_bak_backups` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18373-18377` `store_and_backup_bytes_is_zero_on_a_missing_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18380-18422` `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18425-18490` `footprint_report_measures_every_category_on_a_seeded_fixture_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18493-18510` `footprint_report_folds_a_none_mutation_root_to_a_zero_contribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18513-18595` `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18608-18662` `footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18665-18670` `looks_like_run_container_true_when_every_direct_child_is_a_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18673-18681` `looks_like_run_container_false_when_a_direct_child_is_a_bare_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18684-18692` `looks_like_run_container_is_true_on_an_empty_or_missing_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18695-18708` `classify_agent_scratch_separates_a_bare_top_level_file_from_a_well_formed_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18711-18790` `footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18793-18817` `footprint_report_lines_reports_every_categorys_total_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18820-18831` `footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18835-18843` `assert_hinted_category_is_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18851-18868` `footprint_advisories_flags_a_category_exactly_at_the_threshold_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18871-18881` `footprint_advisories_never_flags_a_category_with_no_reclaim_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18888-18911` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18916-19039` `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19042-19107` `reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19110-19149` `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19152-19182` `reclaim_orphan_scratch_reaps_a_stray_build_cache_tombstone_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19187-19226` `leaked_process_advisories_name_a_process_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19229-19239` `leaked_process_advisories_is_empty_when_no_process_is_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19242-19249` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19254-19260` `parse_result_takes_an_id_and_an_optional_output_arg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19263-19268` `parse_result_with_no_output_defers_to_stdin` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19273-19279` `find_store_dir_from_returns_the_dir_that_holds_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19282-19294` `find_store_dir_from_walks_up_from_a_subdirectory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19297-19303` `git_init_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19306-19343` `find_store_dir_from_never_escapes_the_repo_into_a_parent_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19346-19421` `reap_then_remove_dir_reaps_processes_rooted_inside_then_removes_the_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19424-19473` `reclaim_run_scratch_removes_the_run_level_areas_and_spares_per_unit_scratch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19476-19509` `reclaim_run_scratch_spares_the_shared_build_cache_while_a_build_holds_its_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19512-19523` `find_store_dir_from_refuses_the_worktree_shape_with_no_events_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19526-19555` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19558-19611` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19614-19668` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19671-19698` `walk_stores_from_prefers_the_outermost_store_over_a_nearer_shadow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19701-19719` `walk_stores_from_reports_no_shadow_for_a_single_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19728-19797` `require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19731-19734` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19801-19817` `require_store_dir_fence_is_off_by_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19822-19828` `result_advisories_flags_an_orphan_id_with_no_spawn_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19831-19856` `result_advisories_orphan_wording_is_plain_on_record_and_conditional_under_if_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19859-19869` `result_advisories_is_silent_for_a_parked_unanswered_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19872-19883` `result_advisories_flags_a_supersede_with_the_prior_result_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19886-19899` `result_advisories_suppresses_the_supersede_note_when_not_superseding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19902-19914` `result_advisories_flags_both_orphan_and_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19917-19937` `parse_result_error_flag_is_order_independent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19940-19967` `parse_result_if_absent_is_off_by_default_and_a_bare_order_independent_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19970-20005` `parse_result_meta_must_be_a_json_object` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20008-20022` `parse_result_rejects_missing_id_extra_args_and_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20025-20039` `build_result_shapes_success_and_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20042-20047` `build_result_rejects_a_blank_error_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20050-20059` `build_result_attaches_meta` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20062-20100` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20103-20135` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20141-20231` `scaffold_parses_into_a_valid_config` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20245-20262` `shipped_workflows_carry_a_non_zero_spawn_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20265-20277` `parse_canary_args_defaults_corpus_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20280-20292` `parse_canary_args_reads_corpus_if_model_changed_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20295-20307` `parse_canary_args_rejects_a_non_positive_jobs_value` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20310-20320` `parse_canary_args_rejects_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20323-20352` `usage_text_gives_the_jobs_flag_its_own_description_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20355-20372` `usage_text_names_the_build_cache_reset_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20375-20385` `parse_run_args_defaults_to_cli_and_an_unset_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20388-20399` `parse_run_args_reads_fresh_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20402-20412` `parse_run_args_reads_rebase_definition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20415-20430` `parse_run_args_reads_driver_eventstore_conn_and_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20433-20438` `parse_run_args_rejects_unknown_flags_and_values` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20445-20471` `parse_run_args_accepts_base_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20479-20501` `resolve_run_base_precedence_flag_then_env_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20507-20545` `parse_workflow_args_reads_spec_and_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20550-20602` `parse_step_args_reads_spec_and_base_with_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20608-20675` `definition_hash_is_stable_and_content_sensitive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20684-20711` `kurrentdb_is_always_available_and_needs_a_conn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20723-20754` `store_selection_preserves_a_credentialed_tls_conn_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20765-20942` `store_selection_precedence_flag_env_secret_file_config_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20945-20947` `project_identity_is_never_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20950-20975` `project_identity_reads_the_tracked_id_file_then_falls_back_to_the_basename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20978-21005` `ssh_https_and_git_suffix_forms_of_one_repo_mint_identical_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21008-21022` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21025-21054` `decide_migration_covers_every_case` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21057-21096` `migrate_project_identity_renames_legacy_history_and_records_a_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21099-21138` `migrate_project_identity_refuses_when_both_namespaces_hold_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21141-21206` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21209-21289` `migrate_project_identity_rekeys_the_graph_before_the_irreversible_stream_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21292-21387` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21396-21501` `dash_graph_provider_reaches_the_whole_projection_when_run_seeds_are_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21515-21554` `dash_read_whole_graph_on_an_absent_db_is_empty_and_creates_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21560-21592` `setup_provisions_the_shim_runtime_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21601-21660` `setup_installs_refreshes_and_is_a_noop_on_the_native_rigger_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21665-21674` `outcome_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21678-21683` `registry_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21693-21763` `setup_installs_the_using_rigger_skill_distinct_from_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21781-21865` `planning_a_spec_installs_and_renders_through_the_registry_with_no_planning_specific_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21875-21911` `setup_skill_install_applies_the_project_overlay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21917-21947` `docs_overlay_overrides_only_declared_fields` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21952-21966` `docs_overlay_malformed_is_a_loud_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21976-22004` `provision_shim_is_a_silent_noop_when_already_current` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22014-22033` `shim_is_not_current_when_node_modules_is_torn_missing_the_install_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22040-22062` `init_project_is_idempotent_reporting_new_work_only_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22072-22130` `scaffold_summary_reports_only_the_gitignore_change_on_a_gitignore_only_repair` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22141-22220` `init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22229-22269` `init_project_gitignores_the_store_conn_secret_file_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22278-22292` `a_group_or_other_readable_secret_file_mode_is_flagged_owner_only_is_not` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22304-22352` `init_project_still_writes_the_dash_ignore_lines_when_a_broader_rule_covers_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22361-22423` `scaffold_agents_and_workflow_reference_the_same_canonical_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22430-22473` `init_scaffolds_only_the_workflow_referenced_agents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22481-22528` `init_project_errors_loudly_on_an_unknown_key_and_does_not_scaffold_agents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22535-22564` `get_referenced_agent_ids_reads_the_scaffolded_workflows_fleet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22575-22593` `get_referenced_agent_ids_errors_loudly_on_an_unknown_key_instead_of_returning_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22600-22632` `write_if_absent_wrote_kept_and_errors_naming_the_artifact` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22640-22657` `parse_setup_args_reads_the_agents_directory_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22664-22709` `import_agents_copies_and_normalizes_the_identity_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22715-22752` `import_agents_refuses_to_overwrite_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22758-22771` `import_agents_validates_and_rejects_a_malformed_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22780-22802` `import_agents_rejects_an_id_colliding_with_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22808-22828` `import_agents_rejects_a_duplicate_id_within_one_import` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22834-22855` `import_agents_rejects_an_agent_with_a_blank_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22862-22884` `import_agents_runs_full_validation_and_rejects_a_broken_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22890-22909` `meta_object_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22918-22932` `meta_description` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22939-22947` `strip_line_comments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22961-23184` `workflow_is_a_thin_courier_driver_with_per_unit_phase_labels` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23193-23204` `the_step_schema_admits_the_attention_array` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23220-23276` `phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23289-23327` `meta_matches_reality_drops_integrate_and_the_unit_stage_construction` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23346-23438` `the_driver_relays_each_attention_entry_as_a_narrator_log_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23445-23458` `merge_hung_attention_does_nothing_when_not_newly_hung` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23466-23478` `merge_hung_attention_defers_to_an_existing_budget_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23487-23523` `merge_hung_attention_lands_in_canonical_position_alongside_other_signals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23541-23589` `workflow_step_courier_prompt_is_foreground_and_honest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23599-23608` `step_courier_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23626-23693` `workflow_step_courier_waits_on_an_auto_backgrounded_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23708-23763` `workflow_driver_guards_a_null_step_before_dereferencing_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23776-23833` `workflow_meta_description_is_a_user_facing_tagline_free_of_plumbing_terms` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23840-23864` `setup_runs_npm_install_or_reports_a_clear_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23870-23895` `workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23906-23949` `format_stats_prints_all_four_metrics` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23955-23971` `format_stats_handles_zeroed_metrics_without_nan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23977-23994` `format_canary_stats_reports_findings_raised_by_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23999-24009` `format_canary_stats_reports_a_zero_findings_count_honestly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24015-24021` `format_canary_stats_omits_the_findings_volume_section_when_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24026-24043` `progress_outcome` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24050-24062` `format_progress_line_names_id_verdict_and_none_when_nothing_caught` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24068-24085` `format_progress_line_reports_a_wrong_verdict_and_every_catching_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24093-24124` `format_canary_stats_renders_na_for_a_tier_with_unattributed_correct_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24131-24150` `format_canary_stats_renders_the_real_zero_when_attribution_was_fully_measured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24159-24178` `format_canary_stats_reports_control_items_and_false_positives` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24186-24201` `format_canary_stats_reports_zero_false_positives_honestly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24208-24237` `format_canary_stats_reports_the_model_pinning_header_when_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24243-24249` `format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24259-24301` `format_stats_surfaces_parallelism_retention_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24308-24348` `format_stats_surfaces_spawn_timing_and_reports_unpaired_separately` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24354-24360` `format_stats_spawn_timing_reports_no_spawns_when_none_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24367-24394` `format_stats_spawn_timing_no_spawns_line_requires_both_empty_and_zero_unpaired` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24401-24416` `format_stats_spawn_timing_all_unpaired_is_not_reported_as_no_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24424-24451` `parallelism_retention_line_is_single_sourced_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24458-24503` `stats_discloses_when_no_verdict_was_recorded_on_this_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24514-24587` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24595-24632` `stats_discloses_cause_split_remainder_when_fewer_causes_than_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24637-24643` `cmd_stats_rejects_extra_arguments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24646-24691` `baseline_run_slice_selects_a_run_by_id_including_a_middle_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24694-24725` `format_stats_diff_flags_only_the_changed_rows` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24737-24755` `build_environment_report_with_a_wrapper_lists_wrapper_cache_dir_and_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24762-24779` `build_environment_report_with_no_wrapper_omits_cache_dir_but_keeps_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24785-24795` `build_environment_report_zero_max_concurrent_reports_unlimited` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24801-24810` `build_environment_report_reports_mutation_gate_declared` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24816-24825` `build_environment_report_reports_mutation_gate_not_configured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24843-24862` `order_signature_advisories_names_the_stream_count_range_and_repair_doc` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24866-24868` `order_signature_advisories_is_empty_when_no_signatures_are_given` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24876-24882` `drift_change` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24888-24917` `model_drift_advisory_is_a_soft_note_for_snapshot_only_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24922-24933` `model_drift_advisory_stays_a_warning_when_any_change_is_a_real_model_repoint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24937-24939` `model_drift_advisory_is_none_when_nothing_changed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24944-24960` `index_staleness_message_names_every_kind_of_disagreement_and_the_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24967-24979` `graph_index_lag_advisory_names_every_lagging_file_and_the_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24982-24984` `graph_index_lag_advisory_is_none_when_the_sample_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24987-25010` `bloat_advisory_is_none_at_or_below_the_threshold_and_named_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25018-25033` `assert_advisory_for_never_fabricates_a_missing_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25036-25038` `bloat_advisory_for_never_fabricates_a_store_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25043-25061` `retired_entities_advisory_is_none_at_zero_and_named_with_correct_pluralization` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25064-25069` `retired_entities_advisory_for_never_fabricates_a_graph_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25072-25106` `retired_entities_advisory_for_reads_the_projectors_own_counting_authority` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25113-25120` `scaffold_workflow_declares_build_wrapper_auto` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25128-25141` `scaffold_workflow_declares_max_parallel_units_two_with_a_sizing_comment` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25147-25160` `init_project_writes_max_parallel_units_with_its_sizing_comment` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25167-25174` `defaults_max_parallel_units_is_unbounded_when_the_key_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25182-25196` `init_project_never_clobbers_an_existing_build_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25199-25216` `parse_replay_args_requires_a_run_and_a_rev_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25228-25243` `cmd_stats_on_a_never_run_project_says_no_runs_and_creates_no_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25251-25292` `a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25298-25301` `no_runs_message_points_at_rigger_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25307-25314` `seed_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25320-25332` `stats_lines_absent_db_returns_none_and_creates_no_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25339-25355` `stats_lines_existing_db_with_empty_run_stream_returns_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25363-25392` `stats_lines_does_not_read_another_projects_namespaced_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25399-25430` `stats_lines_existing_run_renders_metric_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25453-25522` `stats_lines_pairs_recorded_spawn_request_and_result_into_spawn_timing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25530-25542` `result_of_at_absent_db_reads_as_unreported_and_creates_no_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25548-25568` `result_of_at_unrecorded_spawn_reads_as_unreported` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25576-25600` `result_of_at_reads_a_self_reported_result_so_it_is_not_clobbered` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25606-25635` `result_of_at_is_namespace_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25642-25654` `cmd_reported_requires_exactly_one_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25667-25692` `detach_process_group_places_the_child_in_its_own_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25701-25722` `a_child_spawned_without_detachment_inherits_the_parent_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25736-25756` `spawn_run_dashboard_detached_session_detaches_the_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25771-25791` `report_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25802-25825` `a_compaction_that_failed_after_the_deletes_is_reported_beside_the_counts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25836-25859` `a_prune_that_shed_nothing_is_justified_by_this_log_not_by_when_it_was_written` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25870-25881` `a_pass_that_deleted_nothing_but_reclaimed_space_reports_the_reclamation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25886-25917` `a_prune_that_shed_rows_explains_the_duplication_a_deduplicated_log_still_accumulates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25931-25966` `an_unmeasurable_database_is_not_reported_as_a_checkpoint_a_reader_declined` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25970-25972` `no_live_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:25976-25994` `live_writer_reasons_is_empty_only_when_all_four_facts_are_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26000-26019` `refusal_names_a_held_step_lock_and_the_force_live_override_owning_the_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26024-26032` `refusal_names_a_non_terminal_unit_between_spawn_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26036-26048` `refusal_names_every_in_flight_spawn_id_and_the_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26052-26059` `refusal_names_the_driver_registration_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26064-26076` `refusal_names_every_applicable_reason_together_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26082-26111` `refuse_derived_reset_if_live_fails_safe_on_a_malformed_spawn_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26117-26138` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26145-26167` `reset_modes_parses_scratch_orphans_alone_and_composed_and_rejects_duplicates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26170-26187` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26195-26204` `watch_test_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26227-26260` `store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_is_found` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26274-26305` `scratch_defaults_reads_the_owning_roots_config_with_no_agents_fleet_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26308-26333` `watch_once_on_a_clean_store_reports_no_anomalies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26336-26340` `parse_watch_args_defaults_to_streaming_with_the_default_interval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26347-26369` `parse_watch_args_accepts_once_and_interval_together_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26372-26376` `parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26384-26504` `watch_once_on_the_seeded_store_reports_one_line_per_anomaly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26511-26530` `the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26538-26566` `watch_once_reports_dash_not_serving_when_the_marker_names_a_dead_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26573-26603` `watch_once_reports_no_anomaly_when_the_dash_marker_names_a_real_serving_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26608-26631` `runs_menu_line_names_the_measured_counts_and_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26634-26663` `derived_menu_line_sums_the_measured_duplicate_counts_and_names_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26670-26685` `derived_menu_line_on_a_server_backend_says_so_instead_of_a_fabricated_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26704-26797` `implementer_persona_pins_the_checkin_stage_kill_or_justify_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26805-26847` `implementer_persona_pins_the_checkpoint_before_long_work_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26858-26888` `no_persona_under_rigger_agents_invokes_cargo_mutants` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26901-26936` `install_operator_mcp_installs_refreshes_and_is_a_noop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26942-26980` `install_lookup_hook_installs_refreshes_and_is_a_noop_and_preserves_foreign_hooks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:26988-27012` `assert_allows_with_literal_stripped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27026-27053` `grep_guard_decision_denies_every_bash_grep_target_and_passes_literal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27059-27072` `grep_guard_decision_allows_non_grep_bash_commands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27081-27089` `grep_guard_decision_denies_every_grep_tool_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27093-27098` `grep_guard_decision_ignores_other_tools` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27109-27125` `grep_guard_decision_denies_a_shell_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27129-27132` `assert_literal_grep_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27152-27165` `grep_guard_decision_denies_a_redirect_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27170-27181` `grep_guard_decision_literal_survives_a_redirect_metacharacter_fused_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27188-27201` `grep_guard_decision_denies_a_quoted_or_escaped_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27207-27220` `grep_guard_decision_literal_survives_a_quoted_literal_on_a_quoted_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27227-27236` `grep_guard_decision_denies_a_grep_split_by_a_line_continuation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27250-27262` `grep_guard_decision_denies_a_path_qualified_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27266-27275` `grep_guard_decision_literal_survives_a_path_qualified_grep` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27280-27289` `grep_guard_decision_allows_a_path_qualified_non_grep_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27294-27302` `guard_write_target_reads_edit_and_write_file_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27305-27313` `guard_write_target_reads_notebook_edit_notebook_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27316-27324` `guard_write_target_ignores_every_other_tool` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27330-27339` `guard_write_target_reports_target_unreadable_for_a_covered_tool_missing_the_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27342-27356` `guard_write_target_reports_target_unreadable_for_a_non_string_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27365-27380` `guard_write_read_target_denies_a_non_object_top_level_value` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27383-27409` `guard_write_read_target_denies_an_absent_or_non_string_tool_name` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27412-27423` `guard_write_read_target_reaches_not_covered_only_through_a_real_string_tool_name` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27426-27441` `guard_write_read_target_denies_a_covered_tool_with_missing_or_non_object_tool_input` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27444-27456` `guard_write_read_target_reads_a_well_formed_covered_payload` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27459-27467` `guard_write_read_target_defaults_an_absent_cwd_to_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27470-27482` `resolve_write_target_passes_an_absolute_path_through` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27485-27490` `assert_resolves_under_cwd` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27498-27510` `resolve_write_target_walks_dot_dot_past_the_process_root_without_panicking` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27513-27531` `resolve_write_target_follows_a_symlinked_ancestor_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27534-27547` `write_target_under_root_rejects_a_sibling_whose_name_merely_shares_a_prefix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27550-27558` `write_target_under_root_treats_equal_as_under` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27561-27570` `write_target_under_root_accepts_a_descendant_and_rejects_a_sibling` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27573-27590` `guard_write_decision_allows_a_target_under_any_configured_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27593-27614` `guard_write_decision_denies_outside_every_root_naming_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27617-27628` `guard_write_decision_denies_a_dot_dot_escape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27631-27637` `guard_write_decision_allows_a_tool_this_guard_does_not_cover` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27643-27653` `guard_write_decision_denies_target_unreadable_naming_the_first_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27656-27665` `parse_guard_write_roots_collects_every_root_in_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27668-27670` `parse_guard_write_roots_requires_at_least_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27673-27676` `parse_guard_write_roots_rejects_a_dangling_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:27679-27682` `parse_guard_write_roots_rejects_an_unknown_argument` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).

### Unassigned (189 functions)

- `src/conductor.rs:372-374` `adoption_provenance_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:409-438` `glob_matches` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:592-600` `input_digest` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:823-840` `conflict_resolution_prompt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1092-1094` `conflict_regenerate_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1099-1101` `cached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1106-1111` `clear_attempt` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1122-1138` `integrate_row` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1884-1892` `classify_failure` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1905-1907` `no_result_error` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1920-1928` `strip_failure_marker` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:2013-2025` `replay_trajectory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:2029-2702` `run` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:2781-2873` `compute_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:11800-11809` `verified_evidence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12167-12214` `render_capped_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12261-12271` `recency_by_own_edge` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12422-12425` `plain_node_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12556-12572` `confidence_tier_radius` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12582-12636` `files_reachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:12903-12911` `current_run_spec` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13060-13077` `branch_owner` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13116-13122` `quarantine_branch_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13217-13245` `quarantined_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13282-13287` `postmerge_worktree_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13294-13296` `postmerge_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13303-13308` `same_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13315-13330` `sanitize_for_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13335-13337` `integrates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13520-13526` `implement_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13705-13713` `fan_out_lenses` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13752-13774` `coverage_gap` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13911-13926` `need_satisfied` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:13968-14005` `validate_acyclic` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:165-174` `console_font_response` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:190-200` `free_port_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:256-288` `probe_dash_head` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:302-306` `dash_serving_on` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:415-417` `head_block_ended` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:690-693` `held_port_holder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:735-739` `url_port` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:812-849` `dash_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:861-869` `dash_start_needed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:923-934` `should_reap_singleton` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1367-1480` `build_state` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1871-1877` `parse_call_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1901-1909` `call_edge_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:1941-2027` `calls_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2101-2212` `build_run_tree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2366-2385` `driver_stage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2389-2397` `rollup` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2470-2477` `gates_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2488-2491` `advanced_past_gates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2496-2505` `unit_live_status` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2510-2518` `spec_of` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2545-2568` `event_seed_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2580-2603` `unit_seeds` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2618-2631` `repoint_seed` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2673-2675` `is_console_event` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2687-2696` `console_event_wire` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2703-2711` `console_progress_wire` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2715-2726` `event_view` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2756-2760` `unix_seconds` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2762-2764` `now_unix` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2773-2792` `state_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2798-2806` `events_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2813-2874` `console_snapshot_json` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2878-2880` `live_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2886-2888` `console_page` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:2895-2915` `render_export` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3226-3244` `percent_decode` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3247-3253` `query_param` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3257-3262` `parse_request_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3547-3554` `env_duration_ms` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3569-3580` `write_sse` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/dash.rs:3589-3595` `write_retained_window_gone` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:273-275` `version_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:453-455` `open_sqlite_store` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:459-463` `env_conn` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:548-551` `config_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:747-749` `registry_heartbeat_interval` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:801-840` `register_run_instance` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:871-904` `refresh_registry_entry` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:916-919` `project_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:944-955` `project_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:962-964` `legacy_identity_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:969-982` `legacy_identity_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1003-1011` `has_tracked_project_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1016-1022` `canonical_definition_text` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1037-1077` `definition_hash` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1080-1085` `push_definition_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1108-1145` `enforce_definition_pin` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1154-1179` `normalize_origin_url` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1184-1200` `origin_url_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1206-1211` `mint_project_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1231-1245` `decide_migration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1424-1487` `main` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1672-1674` `usage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1676-1681` `db_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1723-1780` `walk_stores_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1793-1814` `main_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1889-1938` `refuse_unless_one_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2157-2195` `result_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2210-2214` `load_run_config` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2289-2309` `acquire_step_lock` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2997-3010` `merge_hung_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3151-3154` `reap_then_remove_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3165-3179` `reap_then_remove_worktree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3341-3354` `result_of_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3443-3467` `warn_on_run_branch_divergence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3480-3489` `anchor_run_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3513-3527` `refuse_when_base_unreachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3599-3736` `run_cli` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3760-3804` `fresh_run_if_requested` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4135-4152` `load_criteria` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4473-4526` `definition_body` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4553-4597` `locate_definition_extent` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4605-4615` `locate_definition_extent` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4972-5001` `stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5022-5036` `parallelism_retention_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5099-5120` `append_spawn_timing` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5125-5127` `fmt_duration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5155-5265` `append_review_quality` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5294-5310` `canary_stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5462-5506` `model_drift_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5515-5532` `order_signature_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6029-6039` `started_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6060-6093` `candidate_reaches_gate` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6137-6151` `baseline_run_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6154-6158` `run_started_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6165-6211` `materialize_config_at_rev` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6294-6312` `start_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6319-6334` `spawn_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6421-6427` `detach_process_group` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6430-6430` `detach_process_group` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6664-6719` `spawn_run_dashboard_detached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7258-7308` `watch_and_self_reap_on_idle` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7513-7515` `instance_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8178-8184` `release_ready_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8305-8518` `watch_poll` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8673-8679` `runs_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8686-8708` `derived_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8910-8999` `derived_prune_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9312-9316` `graph_node_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9375-9385` `peer_decision_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9389-9398` `json_str_array` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9402-9411` `json_type_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9507-9528` `build_result` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9776-9788` `fold_recorded_result_into_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10033-10066` `build_environment_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10074-10113` `validate_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10120-10146` `index_staleness_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10155-10165` `graph_index_lag_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10218-10227` `retired_entities_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10240-10246` `retired_entities_advisory_for` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10341-10352` `missing_gitsemver_binary_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10364-10384` `behind_the_tree_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10403-10415` `behind_the_tree_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10435-10449` `drift_side` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10498-10519` `uncommitted_rigger_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10527-10542` `dirty_tracked_paths` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10582-10619` `residue_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10630-10647` `leaked_process_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10747-10793` `current_run_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10925-10944` `local_unit_branches` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10952-11013` `scan_residue` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11090-11109` `dir_size_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11131-11141` `build_cache_tombstone_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11240-11253` `human_size` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11485-11495` `dead_spawn_leaf_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11505-11512` `looks_like_run_container` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11718-11730` `owning_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11811-11934` `init_project` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12047-12074` `get_referenced_agent_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12141-12143` `rigger_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12376-12517` `precommit_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12548-12600` `compose_precommit_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13105-13196` `import_agents` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13205-13235` `normalize_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13240-13246` `top_level_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13477-13492` `instructions_in_force_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13560-13575` `select_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13583-13585` `select_reindex_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13755-13780` `grep_guard_decision` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13818-13880` `shell_command_word_spans` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13884-13890` `shell_command_words` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13906-13931` `strip_literal_marker` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13936-13941` `word_basename` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:13952-13954` `command_invokes_grep` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.

## 2. Duplication Catalog

515 clusters (2562 total sites) across `src/` and `tests/`, found by `tests/simplification_audit.rs`'s deterministic normalized-token-shingle Jaccard pass (8-token shingles, threshold 0.72) plus five mandatory mechanical sweeps. Strict definition (spec 85 Goal): any logic present in more than one place anywhere in the codebase is a violation, with no "small enough to duplicate" exemption.

### Mandatory sweeps

- **Command::new call sites**: 265 site(s) - `dup-124a486a7883`
- **/proc-path string literals**: 56 site(s) - `dup-6f592e6f36fa`
- **sqlite Connection::open call sites**: 39 site(s) - `dup-e4cc0ac793e5`
- **.rigger-path string literals**: 673 site(s) - `dup-8da2bd340832`
- **error-shaping helper functions**: 12 site(s) - `dup-83a6d4922163`

### Clusters (30 exact, 440 near, 45 semantic)

#### `dup-7dfa0dd1a6a8` (near, 2 sites)

Proposed home: `blast_radius_eval::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blast_radius_eval.rs:216-231` `width_threshold_is_the_tier_width_nearest_rank_percentile`
- `src/blast_radius_eval.rs:234-242` `full_fraction_spans_all_light_to_collapse`

#### `dup-32656e0a4e12` (near, 3 sites)

Proposed home: `blocker::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blocker.rs:391-404` `reject_recurrence_line_shows_n_over_max`
- `src/blocker.rs:407-423` `reject_recurrence_line_carries_the_recorded_cause`
- `src/blocker.rs:426-446` `reject_recurrence_line_carries_the_latest_of_several_causes`

#### `dup-124a486a7883` (semantic, 265 sites)

Proposed home: `a single injected process-spawn port every Command::new site routes through instead of constructing its own Command`

mandatory sweep: Command::new call sites - 265 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/budget.rs:198-198` `Command::new`
- `src/budget.rs:236-236` `Command::new`
- `src/budget.rs:247-247` `Command::new`
- `src/conductor.rs:14971-14971` `Command::new`
- `src/conductor.rs:14988-14988` `Command::new`
- `src/conductor.rs:15006-15006` `Command::new`
- `src/conductor.rs:15105-15105` `Command::new`
- `src/conductor.rs:15217-15217` `Command::new`
- `src/conductor.rs:15313-15313` `Command::new`
- `src/conductor.rs:15749-15749` `Command::new`
- `src/conductor.rs:15754-15754` `Command::new`
- `src/conductor.rs:22775-22775` `Command::new`
- `src/conductor.rs:26568-26568` `Command::new`
- `src/conductor.rs:27186-27186` `Command::new`
- `src/conductor.rs:27260-27260` `Command::new`
- `src/conductor.rs:28269-28269` `Command::new`
- `src/conductor.rs:28458-28458` `Command::new`
- `src/conductor.rs:32449-32449` `Command::new`
- `src/conductor.rs:32547-32547` `Command::new`
- `src/conductor.rs:32632-32632` `Command::new`
- `src/conductor.rs:32924-32924` `Command::new`
- `src/conductor.rs:33364-33364` `Command::new`
- `src/conductor.rs:33465-33465` `Command::new`
- `src/conductor.rs:33663-33663` `Command::new`
- `src/conductor.rs:33853-33853` `Command::new`
- `src/conductor.rs:34169-34169` `Command::new`
- `src/conductor.rs:34199-34199` `Command::new`
- `src/conductor.rs:34312-34312` `Command::new`
- `src/conductor.rs:34323-34323` `Command::new`
- `src/conductor.rs:34495-34495` `Command::new`
- `src/conductor.rs:35138-35138` `Command::new`
- `src/conductor.rs:38950-38950` `Command::new`
- `src/conductor.rs:39611-39611` `Command::new`
- `src/conductor.rs:40217-40217` `Command::new`
- `src/conductor.rs:40224-40224` `Command::new`
- `src/conductor.rs:40314-40314` `Command::new`
- `src/conductor.rs:40371-40371` `Command::new`
- `src/conductor.rs:40427-40427` `Command::new`
- `src/conductor.rs:40638-40638` `Command::new`
- `src/conductor.rs:40652-40652` `Command::new`
- `src/conductor.rs:40702-40702` `Command::new`
- `src/conductor.rs:40956-40956` `Command::new`
- `src/conductor.rs:41037-41037` `Command::new`
- `src/conductor.rs:41191-41191` `Command::new`
- `src/conductor.rs:41516-41516` `Command::new`
- `src/conductor.rs:41551-41551` `Command::new`
- `src/conductor.rs:42728-42728` `Command::new`
- `src/conductor.rs:42746-42746` `Command::new`
- `src/conductor.rs:42776-42776` `Command::new`
- `src/conductor.rs:42967-42967` `Command::new`
- `src/conductor.rs:43280-43280` `Command::new`
- `src/dash.rs:3834-3834` `Command::new`
- `src/driver/claude_code.rs:202-202` `Command::new`
- `src/driver/cli.rs:45-45` `Command::new`
- `src/gate.rs:749-749` `Command::new`
- `src/gate.rs:758-758` `Command::new`
- `src/gate.rs:1681-1681` `Command::new`
- `src/main.rs:1185-1185` `Command::new`
- `src/main.rs:1794-1794` `Command::new`
- `src/main.rs:3168-3168` `Command::new`
- `src/main.rs:4052-4052` `Command::new`
- `src/main.rs:6177-6177` `Command::new`
- `src/main.rs:6204-6204` `Command::new`
- `src/main.rs:6322-6322` `Command::new`
- `src/main.rs:6451-6451` `Command::new`
- `src/main.rs:10302-10302` `Command::new`
- `src/main.rs:10321-10321` `Command::new`
- `src/main.rs:10499-10499` `Command::new`
- `src/main.rs:10926-10926` `Command::new`
- `src/main.rs:12007-12007` `Command::new`
- `src/main.rs:12622-12622` `Command::new`
- `src/main.rs:12756-12756` `Command::new`
- `src/main.rs:13599-13599` `Command::new`
- `src/main.rs:14948-14948` `Command::new`
- `src/main.rs:14981-14981` `Command::new`
- `src/main.rs:15025-15025` `Command::new`
- `src/main.rs:15094-15094` `Command::new`
- `src/main.rs:15686-15686` `Command::new`
- `src/main.rs:17279-17279` `Command::new`
- `src/main.rs:17326-17326` `Command::new`
- `src/main.rs:17548-17548` `Command::new`
- `src/main.rs:17560-17560` `Command::new`
- `src/main.rs:17730-17730` `Command::new`
- `src/main.rs:19088-19088` `Command::new`
- `src/main.rs:19094-19094` `Command::new`
- `src/main.rs:19196-19196` `Command::new`
- `src/main.rs:19298-19298` `Command::new`
- `src/main.rs:19364-19364` `Command::new`
- `src/main.rs:19370-19370` `Command::new`
- `src/main.rs:19574-19574` `Command::new`
- `src/main.rs:19580-19580` `Command::new`
- `src/main.rs:19594-19594` `Command::new`
- `src/main.rs:19628-19628` `Command::new`
- `src/main.rs:19634-19634` `Command::new`
- `src/main.rs:19651-19651` `Command::new`
- `src/main.rs:23173-23173` `Command::new`
- `src/main.rs:25670-25670` `Command::new`
- `src/main.rs:25704-25704` `Command::new`
- `src/worktree.rs:722-722` `Command::new`
- `src/worktree.rs:1269-1269` `Command::new`
- `src/worktree.rs:1280-1280` `Command::new`
- `src/worktree.rs:1530-1530` `Command::new`
- `src/worktree.rs:2602-2602` `Command::new`
- `src/worktree.rs:3451-3451` `Command::new`
- `src/worktree.rs:4289-4289` `Command::new`
- `src/worktree.rs:5760-5760` `Command::new`
- `src/worktree.rs:6093-6093` `Command::new`
- `src/worktree.rs:6889-6889` `Command::new`
- `src/worktree.rs:6895-6895` `Command::new`
- `tests/adaptive_labels_periphery.rs:87-87` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:1501-1501` `Command::new`
- `tests/adoption_keys_on_criterion_periphery.rs:2415-2415` `Command::new`
- `tests/change_path_revert_periphery.rs:77-77` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:158-158` `Command::new`
- `tests/checkin_mutation_diff_base_periphery.rs:203-203` `Command::new`
- `tests/claude_code_stream_periphery.rs:1025-1025` `Command::new`
- `tests/cli.rs:1534-1534` `Command::new`
- `tests/cli.rs:1595-1595` `Command::new`
- `tests/cli.rs:5614-5614` `Command::new`
- `tests/cli.rs:5839-5839` `Command::new`
- `tests/cli.rs:6031-6031` `Command::new`
- `tests/cli.rs:6281-6281` `Command::new`
- `tests/cli.rs:6443-6443` `Command::new`
- `tests/cli.rs:11754-11754` `Command::new`
- `tests/cli.rs:11764-11764` `Command::new`
- `tests/cli.rs:11796-11796` `Command::new`
- `tests/cli.rs:14561-14561` `Command::new`
- `tests/cli.rs:15326-15326` `Command::new`
- `tests/cli.rs:15379-15379` `Command::new`
- `tests/cli.rs:20502-20502` `Command::new`
- `tests/cli.rs:20571-20571` `Command::new`
- `tests/cli.rs:20644-20644` `Command::new`
- `tests/cli.rs:20725-20725` `Command::new`
- `tests/cli.rs:20901-20901` `Command::new`
- `tests/cli.rs:20961-20961` `Command::new`
- `tests/cli.rs:21069-21069` `Command::new`
- `tests/cli.rs:21097-21097` `Command::new`
- `tests/cli.rs:21218-21218` `Command::new`
- `tests/cli.rs:21278-21278` `Command::new`
- `tests/cli.rs:21327-21327` `Command::new`
- `tests/cli.rs:21365-21365` `Command::new`
- `tests/cli.rs:21427-21427` `Command::new`
- `tests/cli.rs:21506-21506` `Command::new`
- `tests/cli.rs:21564-21564` `Command::new`
- `tests/cli.rs:21610-21610` `Command::new`
- `tests/cli.rs:30014-30014` `Command::new`
- `tests/common/cli.rs:17-17` `Command::new`
- `tests/common/cli.rs:56-56` `Command::new`
- `tests/common/cli.rs:160-160` `Command::new`
- `tests/common/fixtures/conductor.rs:282-282` `Command::new`
- `tests/common/fixtures/git.rs:25-25` `Command::new`
- `tests/common/fixtures/git.rs:45-45` `Command::new`
- `tests/common/fixtures/host.rs:9-9` `Command::new`
- `tests/common/fixtures/host.rs:61-61` `Command::new`
- `tests/common/layer_cli.rs:71-71` `Command::new`
- `tests/common/mod.rs:143-143` `Command::new`
- `tests/common/repo.rs:187-187` `Command::new`
- `tests/common/served.rs:295-295` `Command::new`
- `tests/compiler_pass_stage1_audit.rs:193-193` `Command::new`
- `tests/compiler_pass_stage1_audit.rs:211-211` `Command::new`
- `tests/core_lane_purity_audit.rs:290-290` `Command::new`
- `tests/courier_registry_refresh_boundary_periphery.rs:86-86` `Command::new`
- `tests/courier_registry_refresh_fence_periphery.rs:42-42` `Command::new`
- `tests/dash_release_ready.rs:348-348` `Command::new`
- `tests/dedup_seeding_periphery.rs:339-339` `Command::new`
- `tests/dedup_seeding_periphery.rs:368-368` `Command::new`
- `tests/gate_store_fence_periphery.rs:181-181` `Command::new`
- `tests/gitsemver_derivation.rs:109-109` `Command::new`
- `tests/gitsemver_worktree_periphery.rs:135-135` `Command::new`
- `tests/halted_spawn_wip_recovery_periphery.rs:866-866` `Command::new`
- `tests/heartbeat_write_read_agree_periphery.rs:58-58` `Command::new`
- `tests/hermetic_test_git_audit.rs:215-215` `Command::new`
- `tests/hermetic_test_git_audit.rs:247-247` `Command::new`
- `tests/hermetic_test_git_audit.rs:319-319` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:302-302` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:475-475` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:675-675` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:935-935` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1166-1166` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1372-1372` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1639-1639` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:1749-1749` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2031-2031` `Command::new`
- `tests/integrate_conflict_merge_periphery.rs:2392-2392` `Command::new`
- `tests/land_refused_names_its_paths_periphery.rs:190-190` `Command::new`
- `tests/land_refused_names_its_paths_periphery.rs:197-197` `Command::new`
- `tests/land_refused_names_its_paths_periphery.rs:210-210` `Command::new`
- `tests/land_refused_names_its_paths_periphery.rs:216-216` `Command::new`
- `tests/land_refused_names_its_paths_periphery.rs:222-222` `Command::new`
- `tests/meta_phases_declaration_periphery.rs:83-83` `Command::new`
- `tests/mutation_runner_pdeathsig_periphery.rs:133-133` `Command::new`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:113-113` `Command::new`
- `tests/native_driver_pipelining_behavior.rs:297-297` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:29-29` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:47-47` `Command::new`
- `tests/phase_of_role_mapping_periphery.rs:59-59` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:228-228` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:247-247` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:379-379` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:392-392` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:399-399` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:760-760` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:767-767` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:784-784` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:791-791` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1038-1038` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1193-1193` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1206-1206` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1335-1335` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1395-1395` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1410-1410` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1446-1446` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1453-1453` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1568-1568` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1580-1580` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1616-1616` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1623-1623` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1640-1640` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1647-1647` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1824-1824` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1833-1833` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:1865-1865` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2005-2005` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2012-2012` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2027-2027` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2046-2046` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2271-2271` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2278-2278` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2290-2290` `Command::new`
- `tests/plan_stage_commit_landing_periphery.rs:2306-2306` `Command::new`
- `tests/postmerge_gate_error_cleanup_periphery.rs:63-63` `Command::new`
- `tests/postmerge_gate_modified_file_periphery.rs:89-89` `Command::new`
- `tests/product_binary_authority_periphery.rs:155-155` `Command::new`
- `tests/reap_before_removal_periphery.rs:370-370` `Command::new`
- `tests/regate_landed_on_resume_periphery.rs:296-296` `Command::new`
- `tests/regate_landed_on_resume_periphery.rs:310-310` `Command::new`
- `tests/regate_landed_on_resume_periphery.rs:323-323` `Command::new`
- `tests/relocated_worktree_store_resolution_periphery.rs:39-39` `Command::new`
- `tests/relocated_worktree_store_resolution_periphery.rs:102-102` `Command::new`
- `tests/relocated_worktree_store_resolution_periphery.rs:172-172` `Command::new`
- `tests/reset_build_cache_periphery.rs:249-249` `Command::new`
- `tests/reset_build_cache_periphery.rs:261-261` `Command::new`
- `tests/reset_build_cache_periphery.rs:339-339` `Command::new`
- `tests/reset_build_cache_periphery.rs:357-357` `Command::new`
- `tests/reset_derived_compaction_periphery.rs:2290-2290` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:43-43` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:214-214` `Command::new`
- `tests/reset_derived_live_writer_guard_periphery.rs:223-223` `Command::new`
- `tests/revert_on_base_hook_bypass_periphery.rs:161-161` `Command::new`
- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:197-197` `Command::new`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:191-191` `Command::new`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:250-250` `Command::new`
- `tests/scaffold_grounder_resolves.rs:93-93` `Command::new`
- `tests/scaffold_grounder_resolves.rs:98-98` `Command::new`
- `tests/step_attention_periphery.rs:548-548` `Command::new`
- `tests/step_root_resolution_periphery.rs:170-170` `Command::new`
- `tests/step_root_resolution_periphery.rs:183-183` `Command::new`
- `tests/store_flag_precedence.rs:63-63` `Command::new`
- `tests/store_flag_precedence.rs:82-82` `Command::new`
- `tests/store_resolution.rs:210-210` `Command::new`
- `tests/store_resolution.rs:289-289` `Command::new`
- `tests/store_resolution.rs:317-317` `Command::new`
- `tests/turbovec_retired_cargo_boundary.rs:50-50` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:124-124` `Command::new`
- `tests/validate_footprint_default_scratch_root_periphery.rs:61-61` `Command::new`

#### `dup-15530d0e8b19` (near, 2 sites)

Proposed home: `canary::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:94-113` `to_event`
- `src/canary.rs:185-198` `to_event`

#### `dup-37f6bddf866b` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/canary_store.rs, src/grounder/design/extract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary_store.rs:822-824` `any_finding_is_critical`
- `src/grounder/design/extract.rs:157-159` `is_handbook_path`

#### `dup-53188abd6299` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/canary_store.rs, tests/common/fixtures/config.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary_store.rs:826-831` `with_anchor`
- `tests/common/fixtures/config.rs:6-11` `agent`

#### `dup-ce5e360ed6f9` (near, 2 sites)

Proposed home: `canary_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary_store.rs:841-847` `catches_matches_an_absolute_path_spelling_of_a_repo_relative_anchor`
- `src/canary_store.rs:850-864` `catches_matches_a_segment_boundary_path_suffix_in_either_direction`

#### `dup-9d7ea1906fb1` (near, 2 sites)

Proposed home: `canary_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary_store.rs:1357-1386` `run_canary_shards_independent_items_concurrently_at_the_scheduling_seam`
- `src/canary_store.rs:1389-1422` `run_canary_jobs_cap_bounds_total_concurrent_spawns_across_both_dimensions`

#### `dup-0b5573eeaebe` (exact, 6 sites)

Proposed home: `a new shared module (sites span 5 files: src/community.rs, src/dash.rs, src/eventstore/mod.rs, src/failure.rs, src/metrics.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/community.rs:243-245` `len`
- `src/community.rs:262-264` `is_empty`
- `src/dash.rs:3785-3787` `id`
- `src/eventstore/mod.rs:243-245` `handed`
- `src/failure.rs:225-227` `is_empty`
- `src/metrics.rs:573-575` `adversary_precision`

#### `dup-66994196ec07` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/community.rs, src/eventstore/mod.rs, src/failure.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/community.rs:249-251` `nodes`
- `src/eventstore/mod.rs:380-382` `types`
- `src/failure.rs:230-232` `rules`

#### `dup-a17e83ea2e51` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/concepts.rs, src/ingest.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/concepts.rs:74-76` `is_intent_doc`
- `src/concepts.rs:80-82` `is_label_doc`
- `src/ingest.rs:403-405` `is_derived_index_type`
- `tests/simplification_audit.rs:2161-2163` `is_keyword`

#### `dup-0481ed780703` (near, 8 sites)

Proposed home: `a new shared module (sites span 5 files: src/conductor.rs, src/driver/claude_code.rs, src/spawn.rs, tests/common/fixtures/graph.rs, tests/postmerge_gate_error_cleanup_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:307-309` `review_round_start_key`
- `src/conductor.rs:355-357` `compensation_queued_key`
- `src/conductor.rs:1092-1094` `conflict_regenerate_key`
- `src/driver/claude_code.rs:988-996` `stop_message`
- `src/spawn.rs:140-142` `spawn_id`
- `tests/common/fixtures/graph.rs:162-164` `spoke_id`
- `tests/postmerge_gate_error_cleanup_periphery.rs:55-57` `expected_postmerge_dir`
- `tests/postmerge_gate_error_cleanup_periphery.rs:58-60` `expected_postmerge_branch`

#### `dup-e6404599f437` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/conductor.rs, src/contextgraph/sqlite.rs, src/spawn_store.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:372-374` `adoption_provenance_key`
- `src/conductor.rs:385-387` `quarantine_record_key`
- `src/contextgraph/sqlite.rs:1908-1910` `code_entity_id`
- `src/spawn_store.rs:62-64` `what`
- `tests/no_os_kill_audit.rs:52-54` `join`

#### `dup-4f8625f176c2` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:565-567` `unit_of_gate_key`
- `src/spawn.rs:227-229` `unit_of`

#### `dup-2f724ff9616c` (near, 17 sites)

Proposed home: `a new shared module (sites span 11 files: src/conductor.rs, src/eventstore/namespace.rs, src/grounder/mod.rs, src/grounder/workflowdef.rs, src/main.rs, src/spawn.rs, src/worktree.rs, tests/canary_model_drift_periphery.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/regate_landed_on_resume_periphery.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:673-675` `deferred_gate_verdict_key`
- `src/conductor.rs:684-686` `deferred_gate_failed_key`
- `src/conductor.rs:12028-12035` `review_protocol`
- `src/eventstore/namespace.rs:69-71` `prefix_for`
- `src/grounder/mod.rs:259-265` `retired_grounder_error`
- `src/grounder/workflowdef.rs:30-32` `stage_id`
- `src/grounder/workflowdef.rs:34-36` `gate_id`
- `src/grounder/workflowdef.rs:38-40` `agent_id`
- `src/main.rs:12225-12227` `skill_source_rel`
- `src/main.rs:13422-13428` `spec_lint_next_step`
- `src/spawn.rs:115-117` `lens_role`
- `src/spawn.rs:193-195` `speculation_group_id`
- `src/worktree.rs:1793-1795` `shared_build_cache_guard_path`
- `tests/canary_model_drift_periphery.rs:112-114` `prose_claiming`
- `tests/halted_spawn_wip_recovery_periphery.rs:134-136` `unit_branch`
- `tests/regate_landed_on_resume_periphery.rs:73-75` `unit_branch`
- `tests/reset_derived_compaction_periphery.rs:2507-2509` `derived_key_for`

#### `dup-32250d38007f` (semantic, 2 sites)

Proposed home: `conductor::review_outcome - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:911-919` `approved`
- `src/conductor.rs:920-928` `rejected`

#### `dup-0fcb9d9d58f0` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:1153-1175` `pending_landing_from_log`
- `src/conductor.rs:1190-1215` `landed_from_log`

#### `dup-f5686c66da77` (near, 6 sites)

Proposed home: `conductor::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:1550-1552` `is_parked`
- `src/conductor.rs:1623-1625` `is_budget_refused`
- `src/conductor.rs:1701-1703` `is_degenerate_reviewer`
- `src/conductor.rs:1718-1720` `is_verdict_channel_mismatch`
- `src/conductor.rs:1762-1764` `is_plan_landing_failed`
- `src/conductor.rs:1785-1787` `is_land_refused`

#### `dup-acc2ed196b43` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:1839-1854` `as_str`
- `src/spec.rs:167-173` `name`

#### `dup-83a6d4922163` (semantic, 12 sites)

Proposed home: `one error-shaping helper module`

mandatory sweep: error-shaping helper functions - 12 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:1905-1907` `no_result_error`
- `src/conductor.rs:5066-5084` `guard_review_round_tree_on_tier_err`
- `src/conductor.rs:28552-28610` `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`
- `src/grounder/mod.rs:259-265` `retired_grounder_error`
- `src/worktree.rs:3151-3178` `land_reports_a_generic_error_for_a_refusal_that_is_neither_tip_moved_nor_blocked`
- `src/worktree.rs:4599-4653` `revert_on_base_aborts_and_errors_on_a_conflicting_revert`
- `tests/adoption_keys_on_criterion_periphery.rs:2332-2504` `a_quarantine_record_whose_ref_was_since_deleted_hard_errors_instead_of_silently_starting_fresh`
- `tests/batched_fold_cadence.rs:174-252` `append_and_fold_batch_is_best_effort_on_fold_error_and_a_no_op_on_an_empty_batch`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:276-364` `run_canary_runs_every_item_to_completion_even_when_one_items_spawn_errors`
- `tests/claude_code_stream_periphery.rs:281-312` `a_child_that_exits_before_init_classifies_unknown_and_carries_the_stderr_tail`
- `tests/grounder_name_contract.rs:40-171` `public_name_contract_predicate_error_and_resolver_agree`
- `tests/integrate_conflict_merge_periphery.rs:1654-1762` `a_non_content_merge_failure_surfaces_as_a_run_error_leaving_branches_intact`

#### `dup-535941bb5619` (semantic, 2 sites)

Proposed home: `one shared `append_and_fold_batch` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:3302-3324` `append_and_fold_batch`
- `src/ingest.rs:46-86` `append_and_fold_batch`

#### `dup-212931e1642a` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/contextgraph/query.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3930-3932` `spawn_is_recorded`
- `src/contextgraph/query.rs:455-457` `is_shared`

#### `dup-33a58a041083` (semantic, 2 sites)

Proposed home: `one shared `effective_review_panel` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:4510-4512` `effective_review_panel`
- `src/config.rs:936-942` `effective_review_panel`

#### `dup-21f642b8fe3c` (near, 3 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10139-10153` `record_merge_attempt`
- `src/conductor.rs:10185-10200` `record_landing_intent`
- `src/conductor.rs:10210-10225` `record_landed`

#### `dup-9b50f55fa65b` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:12853-12855` `unit_branch`
- `src/eventstore/sqlite.rs:718-720` `key_expr`

#### `dup-00aa5624b0f1` (semantic, 2 sites)

Proposed home: `one shared `unit_worktree_dir` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:12873-12879` `unit_worktree_dir`
- `tests/halted_spawn_wip_recovery_periphery.rs:130-132` `unit_worktree_dir`

#### `dup-3f1ef3fa3676` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:13255-13260` `review_worktree_dir`
- `src/conductor.rs:13267-13269` `review_branch`
- `src/conductor.rs:13282-13287` `postmerge_worktree_dir`
- `src/conductor.rs:13294-13296` `postmerge_branch`

#### `dup-6b7ebc7d745b` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14411-14417` `compensated`
- `src/conductor.rs:14422-14427` `plain_failure`

#### `dup-99369c42d138` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14430-14451` `prior_criterion_unit_never_adopts_across_two_different_specs_sharing_the_same_criterion_id`
- `src/conductor.rs:14493-14510` `prior_criterion_unit_a_plain_non_compensation_failure_never_reopens_an_integrated_criterion`

#### `dup-ed66e478154e` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14454-14469` `prior_criterion_unit_still_adopts_across_two_runs_of_the_same_spec`
- `src/conductor.rs:14472-14490` `prior_criterion_unit_readopts_after_a_compensation_reverts_the_integration`

#### `dup-de33c52cc0f7` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:15043-15121` `a_dirty_tree_whose_named_spawn_was_never_requested_gets_no_wip_recovery_commit`
- `src/conductor.rs:15124-15233` `a_dirty_tree_gets_no_wip_recovery_commit_while_a_sibling_spawn_of_the_unit_is_still_live`
- `src/conductor.rs:15236-15329` `a_prior_runs_leftover_spawn_request_for_a_same_named_unit_never_halts_a_new_run`

#### `dup-bbde86d7c01f` (near, 4 sites)

Proposed home: `conductor::stub`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:15592-15599` `prompts_for`
- `src/conductor.rs:15602-15609` `dirs_for`
- `src/conductor.rs:15613-15619` `system_prompt_for`
- `src/conductor.rs:15623-15625` `title_for`

#### `dup-42e20190c39c` (near, 13 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/eventstore/mod.rs, tests/common/real_driver_spy.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:15635-15637` `spawn_ids`
- `src/conductor.rs:37487-37489` `calls`
- `src/conductor.rs:37490-37492` `targets`
- `src/conductor.rs:37493-37495` `mutants_dirs`
- `src/conductor.rs:37496-37498` `store_fences`
- `src/conductor.rs:37499-37501` `build_cache_guards`
- `src/conductor.rs:37502-37504` `build_cache_dirs`
- `src/conductor.rs:37624-37626` `calls`
- `src/eventstore/mod.rs:255-257` `last`
- `src/eventstore/mod.rs:478-480` `recv`
- `src/eventstore/mod.rs:488-490` `try_recv`
- `src/eventstore/mod.rs:493-495` `err`
- `tests/common/real_driver_spy.rs:35-37` `outputs`

#### `dup-a3fe93ea3850` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:15641-15647` `spawned`
- `src/eventstore/mod.rs:386-388` `covers`

#### `dup-65bacee881b1` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:15836-15860` `coverage_gate_refuses_an_uncovered_criterion`
- `src/conductor.rs:30707-30739` `coverage_gap_flags_a_spec_defect_and_errors`
- `src/conductor.rs:31022-31056` `planner_leaving_a_gap_flags_a_spec_defect`
- `src/conductor.rs:31059-31093` `gate_only_stage_is_a_coverage_proxy_gap`

#### `dup-c6d8f0e34b48` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17128-17224` `a_later_episodes_proposal_supersedes_an_earlier_episodes_planner_unit`
- `src/conductor.rs:17287-17366` `a_planner_proposal_with_no_data_episode_field_still_supersedes_via_meta_spawn`

#### `dup-a339d2be67ad` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17369-17496` `a_same_id_refine_restamps_its_episode_so_its_own_episodes_sibling_does_not_reap_it`
- `src/conductor.rs:17499-17621` `a_same_id_refine_survives_its_own_episodes_sibling_add_walked_first`
- `src/conductor.rs:17928-18022` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later`

#### `dup-c1b367529b91` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17658-17676` `append_one`
- `src/conductor.rs:17789-17806` `append_legacy`
- `src/conductor.rs:17808-17826` `append_identified`

#### `dup-30420471154e` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:18127-18244` `harvest_proposed_gates_every_case_with_the_templates_list_unioned`
- `src/conductor.rs:18247-18327` `harvest_proposed_gate_inheritance_survives_a_resumed_window`

#### `dup-7367c5f0f4e7` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:18365-18421` `a_verbatim_copy_still_supersedes_its_baseline`
- `src/conductor.rs:18424-18498` `a_paraphrased_proposal_matches_its_baseline_by_id_not_prose`
- `src/conductor.rs:18501-18585` `a_bracketed_id_echo_with_a_paraphrase_still_supersedes_exactly_once`
- `src/conductor.rs:18588-18660` `a_stale_non_matching_id_falls_back_to_the_verbatim_prose_match`

#### `dup-8da2bd340832` (semantic, 673 sites)

Proposed home: `one .rigger-relative path-composition helper`

mandatory sweep: .rigger-path string literals - 673 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:18935-18935` `"the repo's own .rigger config must load"`
- `src/config_store.rs:41-41` `".rigger"`
- `src/config_store.rs:60-60` `".rigger"`
- `src/config_store.rs:348-348` `".rigger"`
- `src/config_store.rs:349-349` `"create .rigger/instructions"`
- `src/config_store.rs:402-402` `".rigger"`
- `src/config_store.rs:403-403` `".rigger"`
- `src/config_store.rs:427-427` `"examples/demo/.rigger"`
- `src/config_store.rs:428-428` `".rigger"`
- `src/config_store.rs:1394-1394` `".rigger/agents/sdet-author.md"`
- `src/config_store.rs:1395-1395` `"the shipped .rigger/agents/sdet-author.md must exist"`
- `src/config_store.rs:1422-1422` `".rigger/agents/sdet.md"`
- `src/config_store.rs:1423-1423` `"the shipped .rigger/agents/sdet.md must exist"`
- `src/config_store.rs:1918-1918` `".rigger"`
- `src/config_store.rs:1919-1919` `"create .rigger dir"`
- `src/config_store.rs:1951-1951` `".rigger"`
- `src/config_store.rs:1952-1952` `"create .rigger dir"`
- `src/config_store.rs:1973-1973` `".rigger"`
- `src/config_store.rs:1974-1974` `"create .rigger dir"`
- `src/contextgraph/sqlite.rs:4429-4429` `".rigger/workflow.yml"`
- `src/contextgraph/sqlite.rs:4437-4437` `".rigger/workflow.yml"`
- `src/contextgraph/sqlite.rs:4445-4445` `".rigger/workflow.yml"`
- `src/contextgraph/sqlite.rs:4453-4453` `".rigger/workflow.yml"`
- `src/contextgraph/sqlite.rs:4461-4461` `".rigger/workflow.yml"`
- `src/dash.rs:3954-3954` `"{root}/.rigger/events.db"`
- `src/dash.rs:4003-4003` `"/.rigger/events.db"`
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
- `src/gate.rs:502-502` `".rigger-cache-probe-{}"`
- `src/grounder/mod.rs:458-458` `".rigger"`
- `src/grounder/symbols/store.rs:25-25` `".rigger"`
- `src/grounder/symbols/store.rs:35-35` `".rigger"`
- `src/grounder/workflowdef.rs:28-28` `".rigger/workflow.yml"`
- `src/grounder/workflowdef.rs:233-233` `".rigger"`
- `src/grounder/workflowdef.rs:502-502` `".rigger"`
- `src/grounder/workflowdef.rs:590-590` `"this project's own .rigger/workflow.yml must extract at least one event"`
- `src/ingest.rs:820-820` `".rigger"`
- `src/ingest.rs:822-822` `".rigger"`
- `src/ingest.rs:831-831` `"gw/.rigger/workflow.yml@"`
- `src/ingest.rs:849-849` `"one code batch (a.rs) plus one workflow-definition batch (.rigger/workflow.yml) \
             must both advance the shared batch count; got {}"`
- `src/ingest.rs:878-878` `".rigger"`
- `src/ingest.rs:880-880` `".rigger"`
- `src/ingest.rs:914-914` `".rigger"`
- `src/instructions.rs:61-61` `"\nOperator (.rigger/instructions/*.md, filename order):\n"`
- `src/main.rs:72-72` `".rigger"`
- `src/main.rs:595-595` `"the server event store is selected but no connection string is set - provide one via \
         --conn <url>, the KURRENTDB_CONN environment variable, or the .rigger/store.conn \
         secret file"`
- `src/main.rs:1304-1304` `"migrated project history to the durable identity: renamed {n} stream(s) \
                     from the legacy namespace {legacy:?} to the minted identity {minted:?} \
                     (.rigger/{PROJECT_ID_FILE})"`
- `src/main.rs:1371-1371` `"rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE})"`
- `src/main.rs:1494-1494` `"rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
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
- `src/main.rs:4112-4112` `"workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`."`
- `src/main.rs:7218-7218` `".rigger"`
- `src/main.rs:9032-9032` `"a `rigger step` is running right now (it holds .rigger/step.lock)"`
- `src/main.rs:10268-10268` `".rigger-workflow-provenance"`
- `src/main.rs:10512-10512` `"warning: tracked .rigger/ files have uncommitted modifications:"`
- `src/main.rs:11915-11915` `".rigger/shim/"`
- `src/main.rs:11916-11916` `".rigger/dash.url"`
- `src/main.rs:11917-11917` `".rigger/dash.marker"`
- `src/main.rs:11918-11918` `".rigger/dash.attempt"`
- `src/main.rs:11919-11919` `".rigger/store.conn"`
- `src/main.rs:12087-12087` `"minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"`
- `src/main.rs:12092-12092` `"scaffolded .rigger/workflow.yml"`
- `src/main.rs:12095-12095` `"scaffolded .rigger/instructions/README.md"`
- `src/main.rs:12099-12099` `"scaffolded .rigger/agents/{{{}}}"`
- `src/main.rs:12379-12379` `r#"__BEGIN__
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
- `src/main.rs:12825-12825` `"imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)"`
- `src/main.rs:12875-12875` `"provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"`
- `src/main.rs:13156-13156` `"kept existing .rigger/agents/{name} (import never overwrites)"`
- `src/main.rs:13184-13184` `"imported .rigger/agents/{name} (id: {id})"`
- `src/main.rs:14464-14464` `"# Scaffolded by `rigger init`. A worked plan -> implement pipeline where the\n\
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
- `src/main.rs:15806-15806` `"/.rigger/tmp/cargo-target/debug/rigger"`
- `src/main.rs:15807-15807` `"/.rigger/tmp/cargo-target/release/rigger"`
- `src/main.rs:17118-17118` `" M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n"`
- `src/main.rs:17128-17128` `".rigger/workflow.yml"`
- `src/main.rs:17129-17129` `".rigger/agents/sdet.md"`
- `src/main.rs:17130-17130` `".rigger/agents/new.md"`
- `src/main.rs:17131-17131` `".rigger/agents/gone.md"`
- `src/main.rs:18080-18080` `".rigger"`
- `src/main.rs:18084-18084` `".rigger"`
- `src/main.rs:18110-18110` `"probe/.rigger/events.db"`
- `src/main.rs:18111-18111` `"rigger-wt-x/.rigger/events.db"`
- `src/main.rs:18263-18263` `".rigger"`
- `src/main.rs:18298-18298` `"rigger-wt-unit-99-ghost-12345678/.rigger/events.db"`
- `src/main.rs:18333-18333` `"probe/.rigger/events.db"`
- `src/main.rs:18344-18344` `"shadow store: probe/.rigger/events.db (6B)"`
- `src/main.rs:18429-18429` `".rigger"`
- `src/main.rs:18496-18496` `".rigger"`
- `src/main.rs:18564-18564` `".rigger"`
- `src/main.rs:18643-18643` `".rigger"`
- `src/main.rs:18749-18749` `".rigger"`
- `src/main.rs:19320-19320` `".rigger"`
- `src/main.rs:19360-19360` `".rigger"`
- `src/main.rs:19542-19542` `".rigger"`
- `src/main.rs:19553-19553` `"must walk past the storeless worktree `.rigger/` to the repo's real store"`
- `src/main.rs:20610-20610` `".rigger"`
- `src/main.rs:20612-20612` `".rigger"`
- `src/main.rs:20655-20655` `".rigger"`
- `src/main.rs:20690-20690` `".rigger"`
- `src/main.rs:20726-20726` `".rigger"`
- `src/main.rs:20768-20768` `".rigger"`
- `src/main.rs:20874-20874` `".rigger/store.conn beats the committed config"`
- `src/main.rs:21567-21567` `"{name} must be written into .rigger/shim/"`
- `src/main.rs:22124-22124` `".rigger/agents/"`
- `src/main.rs:22150-22150` `".rigger/dash.url"`
- `src/main.rs:22153-22153` `".rigger/dash.marker"`
- `src/main.rs:22156-22156` `".rigger/dash.attempt"`
- `src/main.rs:22165-22165` `".rigger/dash.url"`
- `src/main.rs:22169-22169` `".rigger/dash.marker"`
- `src/main.rs:22173-22173` `".rigger/dash.attempt"`
- `src/main.rs:22184-22184` `".rigger/dash.url"`
- `src/main.rs:22187-22187` `".rigger/dash.marker"`
- `src/main.rs:22190-22190` `".rigger/dash.attempt"`
- `src/main.rs:22199-22199` `".rigger/dash.url"`
- `src/main.rs:22202-22202` `"exactly one .rigger/dash.url ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:22207-22207` `".rigger/dash.marker"`
- `src/main.rs:22210-22210` `"exactly one .rigger/dash.marker ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:22215-22215` `".rigger/dash.attempt"`
- `src/main.rs:22218-22218` `"exactly one .rigger/dash.attempt ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:22238-22238` `".rigger/store.conn"`
- `src/main.rs:22246-22246` `".rigger/store.conn"`
- `src/main.rs:22255-22255` `".rigger/store.conn"`
- `src/main.rs:22264-22264` `".rigger/store.conn"`
- `src/main.rs:22267-22267` `"exactly one .rigger/store.conn ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:22308-22308` `".rigger/\n"`
- `src/main.rs:22314-22314` `".rigger/dash.url"`
- `src/main.rs:22317-22317` `".rigger/dash.marker"`
- `src/main.rs:22320-22320` `".rigger/dash.attempt"`
- `src/main.rs:22321-22321` `"setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}"`
- `src/main.rs:22329-22329` `".rigger/dash.url"`
- `src/main.rs:22330-22330` `".rigger/dash.marker"`
- `src/main.rs:22331-22331` `".rigger/dash.attempt"`
- `src/main.rs:22332-22332` `"all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"`
- `src/main.rs:22342-22342` `".rigger/dash.url"`
- `src/main.rs:22345-22345` `".rigger/dash.marker"`
- `src/main.rs:22348-22348` `".rigger/dash.attempt"`
- `src/main.rs:22692-22692` `".rigger/agents/researcher.md"`
- `src/main.rs:22721-22721` `".rigger/agents/planner.md"`
- `src/main.rs:22751-22751` `".rigger/agents/newcomer.md"`
- `src/main.rs:22799-22799` `".rigger/agents/my-planner.md"`
- `src/main.rs:22824-22824` `".rigger/agents/a-dup.md"`
- `src/main.rs:22825-22825` `".rigger/agents/b-dup.md"`
- `src/main.rs:22852-22852` `".rigger/agents/blank.md"`
- `src/main.rs:22867-22867` `".rigger/workflow.yml"`
- `src/main.rs:23889-23889` `"locate_shim must return the provisioned .rigger/shim/shim.mjs"`
- `src/main.rs:25023-25023` `".rigger"`
- `src/main.rs:25077-25077` `".rigger"`
- `src/reap.rs:721-721` `".rigger"`
- `src/reap.rs:1096-1096` `"a relocated/cache-home-style authorized_root with no .rigger/tmp relationship \
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
- `src/worktree.rs:1705-1705` `"{}/.rigger/tmp"`
- `src/worktree.rs:4964-4964` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:4965-4965` `"the default must no longer nest inside the repo's own .rigger: {dflt:?}"`
- `src/worktree.rs:4968-4968` `"/.rigger/"`
- `src/worktree.rs:4968-4968` `"/.rigger"`
- `src/worktree.rs:4969-4969` `"the default must never live under any .rigger: {dflt:?}"`
- `src/worktree.rs:4988-4988` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:5012-5012` `"~/.rigger-scratch-test"`
- `src/worktree.rs:5013-5013` `"{home}/.rigger-scratch-test"`
- `src/worktree.rs:6827-6827` `"{base}..rigger-run"`
- `src/worktree.rs:6876-6876` `".rigger"`
- `tests/architecture_current_surface.rs:93-93` `".rigger/store.conn"`
- `tests/architecture_current_surface.rs:145-145` `"docs/architecture.md must describe the system that exists today (spec 56, \
         criterion 1): it must name the store-resolution and configuration surface (the \
         committed `store:` selection, the `KURRENTDB_CONN` environment variable, and the \
         per-machine `.rigger/store.conn` secret file) and the graph inspector's real query \
         surface (the three `lens=` names and the directed `view=calls` `dir=` views). \
         Surfaces the document fails to name: {missing:#?}"`
- `tests/canary_model_drift_periphery.rs:62-62` `".rigger"`
- `tests/change_path_revert_periphery.rs:83-83` `".rigger"`
- `tests/checkin_mutation_diff_base_periphery.rs:53-53` `".rigger"`
- `tests/checkpoint_commit_hook_bypass_periphery.rs:58-58` `"{repo_path}/.rigger-test-scratch"`
- `tests/cli.rs:88-88` `".rigger"`
- `tests/cli.rs:92-92` `".rigger"`
- `tests/cli.rs:154-154` `".rigger"`
- `tests/cli.rs:607-607` `".rigger"`
- `tests/cli.rs:718-718` `".rigger"`
- `tests/cli.rs:773-773` `".rigger"`
- `tests/cli.rs:893-893` `".rigger"`
- `tests/cli.rs:923-923` `".rigger"`
- `tests/cli.rs:1080-1080` `".rigger"`
- `tests/cli.rs:1101-1101` `".rigger"`
- `tests/cli.rs:1209-1209` `"/.rigger/"`
- `tests/cli.rs:1210-1210` `"the default must never live under any .rigger, even on the HOME-only fallback \
         rung; got: {stdout:?}"`
- `tests/cli.rs:1264-1264` `"{}/.rigger/tmp"`
- `tests/cli.rs:1276-1276` `"/.rigger/tmp/"`
- `tests/cli.rs:1374-1374` `".rigger"`
- `tests/cli.rs:1384-1384` `".rigger"`
- `tests/cli.rs:1451-1451` `".rigger"`
- `tests/cli.rs:1452-1452` `".rigger"`
- `tests/cli.rs:1464-1464` `".rigger"`
- `tests/cli.rs:1486-1486` `".rigger"`
- `tests/cli.rs:1555-1555` `".rigger"`
- `tests/cli.rs:1593-1593` `".rigger"`
- `tests/cli.rs:1613-1613` `".rigger"`
- `tests/cli.rs:1646-1646` `".rigger"`
- `tests/cli.rs:1661-1661` `".rigger"`
- `tests/cli.rs:2321-2321` `".rigger"`
- `tests/cli.rs:2494-2494` `".rigger"`
- `tests/cli.rs:2535-2535` `".rigger"`
- `tests/cli.rs:2567-2567` `".rigger"`
- `tests/cli.rs:2568-2568` `"graph build must create .rigger/graph.db from a cold checkout"`
- `tests/cli.rs:2618-2618` `".rigger"`
- `tests/cli.rs:2619-2619` `"graph build must create .rigger/graph.db even when there is nothing to ingest"`
- `tests/cli.rs:2642-2642` `".rigger"`
- `tests/cli.rs:2722-2722` `".rigger"`
- `tests/cli.rs:2737-2737` `".rigger"`
- `tests/cli.rs:2787-2787` `".rigger"`
- `tests/cli.rs:2812-2812` `".rigger"`
- `tests/cli.rs:3266-3266` `".rigger"`
- `tests/cli.rs:3270-3270` `"grounding via symbols must persist the structural index to .rigger/symbols/"`
- `tests/cli.rs:3443-3443` `".rigger"`
- `tests/cli.rs:3707-3707` `".rigger"`
- `tests/cli.rs:3852-3852` `".rigger"`
- `tests/cli.rs:4047-4047` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4095-4095` `"cd ${REPO} && CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4282-4282` `".rigger/tmp/agent-scratch/"`
- `tests/cli.rs:4288-4288` `".rigger/tmp/cargo-target"`
- `tests/cli.rs:4296-4296` `"CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:4781-4781` `".rigger"`
- `tests/cli.rs:4884-4884` `".rigger"`
- `tests/cli.rs:5536-5536` `".rigger"`
- `tests/cli.rs:5706-5706` `".rigger"`
- `tests/cli.rs:5776-5776` `".rigger"`
- `tests/cli.rs:5914-5914` `".rigger"`
- `tests/cli.rs:5971-5971` `".rigger"`
- `tests/cli.rs:6177-6177` `".rigger"`
- `tests/cli.rs:6231-6231` `".rigger"`
- `tests/cli.rs:6401-6401` `".rigger"`
- `tests/cli.rs:6507-6507` `".rigger"`
- `tests/cli.rs:6586-6586` `".rigger"`
- `tests/cli.rs:6719-6719` `".rigger"`
- `tests/cli.rs:6776-6776` `".rigger"`
- `tests/cli.rs:6961-6961` `".rigger"`
- `tests/cli.rs:7034-7034` `".rigger"`
- `tests/cli.rs:7140-7140` `".rigger"`
- `tests/cli.rs:7209-7209` `".rigger"`
- `tests/cli.rs:7311-7311` `".rigger"`
- `tests/cli.rs:7379-7379` `".rigger"`
- `tests/cli.rs:7514-7514` `".rigger"`
- `tests/cli.rs:7583-7583` `".rigger"`
- `tests/cli.rs:7714-7714` `".rigger"`
- `tests/cli.rs:7814-7814` `".rigger/events.db"`
- `tests/cli.rs:8385-8385` `"/.rigger/"`
- `tests/cli.rs:8386-8386` `"the default marker path must never live under any .rigger; got: {marker_str:?}"`
- `tests/cli.rs:8963-8963` `"/.rigger/tmp/agent-live/"`
- `tests/cli.rs:8964-8964` `"under RIGGER_TMPDIR the marker is not under the repo's .rigger/tmp; got: {marker_str:?}"`
- `tests/cli.rs:9745-9745` `".rigger"`
- `tests/cli.rs:9908-9908` `".rigger"`
- `tests/cli.rs:10012-10012` `".rigger"`
- `tests/cli.rs:10061-10061` `".rigger"`
- `tests/cli.rs:10163-10163` `".rigger"`
- `tests/cli.rs:10222-10222` `".rigger"`
- `tests/cli.rs:10326-10326` `".rigger"`
- `tests/cli.rs:10393-10393` `".rigger"`
- `tests/cli.rs:10535-10535` `".rigger"`
- `tests/cli.rs:10583-10583` `".rigger"`
- `tests/cli.rs:10701-10701` `".rigger"`
- `tests/cli.rs:10972-10972` `".rigger"`
- `tests/cli.rs:11071-11071` `".rigger"`
- `tests/cli.rs:11160-11160` `".rigger"`
- `tests/cli.rs:11200-11200` `".rigger"`
- `tests/cli.rs:11985-11985` `".rigger"`
- `tests/cli.rs:12364-12364` `".rigger/workflow.yml"`
- `tests/cli.rs:12364-12364` `".rigger/agents"`
- `tests/cli.rs:12457-12457` `".rigger/workflow.yml"`
- `tests/cli.rs:12457-12457` `".rigger/agents"`
- `tests/cli.rs:12459-12459` `".rigger"`
- `tests/cli.rs:12460-12460` `".rigger/workflow.yml"`
- `tests/cli.rs:12520-12520` `".rigger/workflow.yml"`
- `tests/cli.rs:12520-12520` `".rigger/agents"`
- `tests/cli.rs:12522-12522` `".rigger"`
- `tests/cli.rs:12523-12523` `".rigger/workflow.yml"`
- `tests/cli.rs:12591-12591` `".rigger/workflow.yml"`
- `tests/cli.rs:12591-12591` `".rigger/agents"`
- `tests/cli.rs:12593-12593` `".rigger"`
- `tests/cli.rs:12594-12594` `".rigger/workflow.yml"`
- `tests/cli.rs:12660-12660` `".rigger/workflow.yml"`
- `tests/cli.rs:12660-12660` `".rigger/agents"`
- `tests/cli.rs:12662-12662` `".rigger"`
- `tests/cli.rs:12663-12663` `".rigger/workflow.yml"`
- `tests/cli.rs:12718-12718` `".rigger/workflow.yml"`
- `tests/cli.rs:12718-12718` `".rigger/agents"`
- `tests/cli.rs:12765-12765` `".rigger/workflow.yml"`
- `tests/cli.rs:12765-12765` `".rigger/agents"`
- `tests/cli.rs:12778-12778` `".rigger"`
- `tests/cli.rs:12787-12787` `".rigger"`
- `tests/cli.rs:12789-12789` `".rigger"`
- `tests/cli.rs:12886-12886` `".rigger/workflow.yml"`
- `tests/cli.rs:12887-12887` `"validate must NOT flag a clean tracked `.rigger/` tree; stderr:\n{err}"`
- `tests/cli.rs:12896-12896` `".rigger"`
- `tests/cli.rs:12903-12903` `"validate must still succeed (exit 0) when it only FLAGS uncommitted `.rigger/` \
         changes; stderr:\n{err}"`
- `tests/cli.rs:12907-12907` `".rigger/workflow.yml"`
- `tests/cli.rs:12908-12908` `"validate must flag the tracked-but-modified `.rigger/workflow.yml` on stderr; \
         stderr:\n{err}"`
- `tests/cli.rs:12924-12924` `".rigger"`
- `tests/cli.rs:13169-13169` `".rigger"`
- `tests/cli.rs:13391-13391` `".rigger"`
- `tests/cli.rs:13429-13429` `".rigger"`
- `tests/cli.rs:13661-13661` `".rigger"`
- `tests/cli.rs:13805-13805` `".rigger"`
- `tests/cli.rs:13807-13807` `".rigger"`
- `tests/cli.rs:13810-13810` `".rigger"`
- `tests/cli.rs:13812-13812` `".rigger"`
- `tests/cli.rs:13840-13840` `"probe/.rigger/events.db"`
- `tests/cli.rs:13904-13904` `"validate must warn about residue planted under the relocated cache-home DEFAULT \
         root - a regression that left its residue scan still rooted at the pre-relocation \
         `.rigger/tmp` would silently miss this and print nothing; stderr:\n{err}"`
- `tests/cli.rs:13934-13934` `".rigger"`
- `tests/cli.rs:14854-14854` `".rigger"`
- `tests/cli.rs:14951-14951` `"scaffolded .rigger/workflow.yml"`
- `tests/cli.rs:14955-14955` `"scaffolded .rigger/agents/"`
- `tests/cli.rs:14987-14987` `".rigger"`
- `tests/cli.rs:15019-15019` `".rigger/agents/researcher.md"`
- `tests/cli.rs:15020-15020` `"the agent was actually imported into .rigger/agents/"`
- `tests/cli.rs:15067-15067` `".rigger"`
- `tests/cli.rs:15296-15296` `".rigger/dash.url"`
- `tests/cli.rs:15300-15300` `".rigger/dash.marker"`
- `tests/cli.rs:15304-15304` `".rigger/dash.attempt"`
- `tests/cli.rs:15313-15313` `".rigger"`
- `tests/cli.rs:15315-15315` `".rigger"`
- `tests/cli.rs:15319-15319` `".rigger"`
- `tests/cli.rs:15320-15320` `".rigger"`
- `tests/cli.rs:15322-15322` `".rigger/dash.url"`
- `tests/cli.rs:15323-15323` `".rigger/dash.marker"`
- `tests/cli.rs:15324-15324` `".rigger/dash.attempt"`
- `tests/cli.rs:15364-15364` `".claude/\n.rigger/\n"`
- `tests/cli.rs:15380-15380` `".rigger/dash.url"`
- `tests/cli.rs:15388-15388` `"the test's global config must actually ignore .rigger/dash.url (else the regression \
         guard is inconclusive)"`
- `tests/cli.rs:15411-15411` `".rigger/shim"`
- `tests/cli.rs:15412-15412` `".rigger/dash.url"`
- `tests/cli.rs:15413-15413` `".rigger/dash.marker"`
- `tests/cli.rs:15414-15414` `".rigger/dash.attempt"`
- `tests/cli.rs:15618-15618` `".rigger/project.id"`
- `tests/cli.rs:15621-15621` `".rigger/project.id"`
- `tests/cli.rs:15634-15634` `".rigger/project.id"`
- `tests/cli.rs:15723-15723` `".rigger/project.id"`
- `tests/cli.rs:15772-15772` `".rigger/project.id"`
- `tests/cli.rs:15777-15777` `".rigger/project.id"`
- `tests/cli.rs:15791-15791` `".rigger"`
- `tests/cli.rs:15934-15934` `".rigger"`
- `tests/cli.rs:16053-16053` `".rigger"`
- `tests/cli.rs:16145-16145` `".rigger"`
- `tests/cli.rs:16210-16210` `".rigger"`
- `tests/cli.rs:16280-16280` `".rigger"`
- `tests/cli.rs:16644-16644` `".rigger"`
- `tests/cli.rs:16712-16712` `".rigger"`
- `tests/cli.rs:16849-16849` `".rigger"`
- `tests/cli.rs:17048-17048` `".rigger"`
- `tests/cli.rs:17238-17238` `".rigger"`
- `tests/cli.rs:17420-17420` `".rigger"`
- `tests/cli.rs:17465-17465` `".rigger"`
- `tests/cli.rs:17903-17903` `".rigger"`
- `tests/cli.rs:17918-17918` `"the driver never recorded a dash URL in .rigger/dash.url; stderr:\n{err}"`
- `tests/cli.rs:17963-17963` `".rigger"`
- `tests/cli.rs:18026-18026` `".rigger"`
- `tests/cli.rs:18101-18101` `".rigger"`
- `tests/cli.rs:18177-18177` `".rigger"`
- `tests/cli.rs:18261-18261` `".rigger"`
- `tests/cli.rs:18368-18368` `".rigger"`
- `tests/cli.rs:19084-19084` `".rigger"`
- `tests/cli.rs:19086-19086` `".rigger"`
- `tests/cli.rs:19728-19728` `".rigger/dash.marker"`
- `tests/cli.rs:19769-19769` `".rigger/dash.marker"`
- `tests/cli.rs:19772-19772` `".rigger/dash.url"`
- `tests/cli.rs:19824-19824` `".rigger/dash.url"`
- `tests/cli.rs:19829-19829` `".rigger/dash.marker"`
- `tests/cli.rs:19885-19885` `".rigger/dash.url"`
- `tests/cli.rs:19887-19887` `".rigger/dash.marker"`
- `tests/cli.rs:19956-19956` `".rigger/dash.marker"`
- `tests/cli.rs:20014-20014` `".rigger/dash.marker"`
- `tests/cli.rs:20119-20119` `".rigger/dash.marker"`
- `tests/cli.rs:20173-20173` `".rigger/dash.marker"`
- `tests/cli.rs:20195-20195` `".rigger/dash.attempt"`
- `tests/cli.rs:20205-20205` `"a marker that LOOKS like it predates this run's own RunStarted must still be reported \
         when .rigger/dash.attempt explicitly names this exact run - proving watch_poll's own \
         file-read-and-match wiring (not merely the pure watch::detect fallback comparison, \
         which alone would suppress this exact shape) is what forced the report; got:\n{out}"`
- `tests/cli.rs:20253-20253` `".rigger/dash.marker"`
- `tests/cli.rs:20275-20275` `".rigger/dash.attempt"`
- `tests/cli.rs:21001-21001` `".rigger"`
- `tests/cli.rs:21816-21816` `".rigger"`
- `tests/cli.rs:22157-22157` `".rigger"`
- `tests/cli.rs:22192-22192` `".rigger"`
- `tests/cli.rs:22267-22267` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:22348-22348` `"the step must record a dash marker at .rigger/dash.marker; stderr:\n{err}"`
- `tests/cli.rs:22362-22362` `"`rigger step` under RIGGER_DASH_PORT={dash_port} must bind its step-path dash at EXACTLY \
         that port and record it in .rigger/dash.marker (proving the override reaches the real \
         bind, not the fixed dash::DEFAULT_PORT); the marker instead recorded {marker_port}"`
- `tests/cli.rs:22423-22423` `".rigger"`
- `tests/cli.rs:22504-22504` `".rigger"`
- `tests/cli.rs:22575-22575` `".rigger"`
- `tests/cli.rs:22947-22947` `".rigger"`
- `tests/cli.rs:22959-22959` `".rigger"`
- `tests/cli.rs:22986-22986` `".rigger"`
- `tests/cli.rs:23014-23014` `".rigger"`
- `tests/cli.rs:23025-23025` `".rigger"`
- `tests/cli.rs:23062-23062` `".rigger"`
- `tests/cli.rs:23151-23151` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:23223-23223` `"{root}/.rigger/events.db"`
- `tests/cli.rs:23240-23240` `"{root}/.rigger/events.db"`
- `tests/cli.rs:24131-24131` `"{other_root}/.rigger/events.db"`
- `tests/cli.rs:24614-24614` `"/stale/root/.rigger/events.db"`
- `tests/cli.rs:24635-24635` `"/live/root/.rigger/events.db"`
- `tests/cli.rs:24717-24717` `"the step must record a dash marker at .rigger/dash.marker"`
- `tests/cli.rs:24917-24917` `".rigger"`
- `tests/cli.rs:25099-25099` `".rigger"`
- `tests/cli.rs:25202-25202` `".rigger"`
- `tests/cli.rs:25327-25327` `".rigger"`
- `tests/cli.rs:25968-25968` `".rigger/workflow.yml"`
- `tests/cli.rs:25972-25972` `".rigger/workflow.yml must define a `checkin:` stage (spec 91): {text:?}"`
- `tests/cli.rs:25976-25976` `".rigger/workflow.yml must define a `mutation:` gate that invokes cargo mutants \
         (spec 91): {text:?}"`
- `tests/cli.rs:25981-25981` `".rigger/workflow.yml's checkin stage / mutation gate definition must name spec 91, \
         so drift in the committed workflow fails this suite instead of silently diverging \
         from the spec it satisfies: {text:?}"`
- `tests/cli.rs:26007-26007` `"this repository's own .rigger/workflow.yml and agents must load: {e}"`
- `tests/cli.rs:26014-26014` `".rigger/workflow.yml must define a `checkin` stage (spec 91)"`
- `tests/cli.rs:26049-26049` `".rigger/workflow.yml must define a `mutation` gate (spec 91)"`
- `tests/cli.rs:26070-26070` `"this repository's own committed .rigger/workflow.yml must pass Config::validate \
         on a correctly-provisioned machine (cargo-mutants installed)"`
- `tests/cli.rs:26115-26115` `".rigger/dash.attempt"`
- `tests/cli.rs:26116-26116` `"a real step's own ensure_run_dashboard call must record .rigger/dash.attempt \
         (record_dash_attempt); without it this test cannot exercise the round-8 fact at all"`
- `tests/cli.rs:26173-26173` `".rigger/dash.url"`
- `tests/cli.rs:26181-26181` `".rigger/dash.marker"`
- `tests/cli.rs:26245-26245` `".rigger/dash.marker"`
- `tests/cli.rs:26274-26274` `".rigger/dash.url"`
- `tests/cli.rs:26281-26281` `".rigger/dash.attempt"`
- `tests/cli.rs:26342-26342` `".rigger/dash.marker"`
- `tests/cli.rs:26345-26345` `".rigger/dash.url"`
- `tests/cli.rs:26399-26399` `".rigger/dash.marker"`
- `tests/cli.rs:26421-26421` `".rigger/dash.url"`
- `tests/cli.rs:26426-26426` `".rigger/dash.attempt"`
- `tests/cli.rs:26465-26465` `".rigger/dash.url"`
- `tests/cli.rs:26476-26476` `".rigger/dash.marker"`
- `tests/cli.rs:26512-26512` `".rigger/dash.url"`
- `tests/cli.rs:26520-26520` `".rigger/dash.marker"`
- `tests/cli.rs:26572-26572` `".rigger/dash.url"`
- `tests/cli.rs:26577-26577` `".rigger/dash.marker"`
- `tests/cli.rs:26639-26639` `".rigger/dash.url"`
- `tests/cli.rs:26647-26647` `".rigger/dash.marker"`
- `tests/cli.rs:26900-26900` `".rigger"`
- `tests/cli.rs:27608-27608` `".rigger"`
- `tests/cli.rs:27646-27646` `".rigger"`
- `tests/cli.rs:28643-28643` `".rigger"`
- `tests/cli.rs:28668-28668` `".rigger"`
- `tests/cli.rs:28783-28783` `".rigger"`
- `tests/cli.rs:29005-29005` `".rigger"`
- `tests/cli.rs:29575-29575` `"the hook must be inert on a project without .rigger/; got:\n{out}"`
- `tests/cli.rs:29581-29581` `".rigger"`
- `tests/cli.rs:29600-29600` `".rigger"`
- `tests/cli.rs:29626-29626` `".rigger"`
- `tests/cli.rs:29662-29662` `".rigger"`
- `tests/cli.rs:29711-29711` `".rigger"`
- `tests/cli.rs:29749-29749` `".rigger"`
- `tests/cli.rs:29783-29783` `".rigger"`
- `tests/cli.rs:29808-29808` `".rigger"`
- `tests/cli.rs:29839-29839` `".rigger"`
- `tests/cli.rs:29860-29860` `".rigger"`
- `tests/cli.rs:29882-29882` `".rigger"`
- `tests/cli.rs:29900-29900` `".rigger"`
- `tests/cli.rs:29928-29928` `".rigger"`
- `tests/cli.rs:29965-29965` `".rigger"`
- `tests/cli.rs:30004-30004` `".rigger"`
- `tests/cli.rs:30043-30043` `".rigger"`
- `tests/cli.rs:30351-30351` `".rigger"`
- `tests/cli.rs:30555-30555` `".rigger"`
- `tests/cli.rs:30605-30605` `".rigger"`
- `tests/cli.rs:30636-30636` `"scaffolded .rigger/instructions/README.md"`
- `tests/cli.rs:30640-30640` `".rigger/instructions/README.md"`
- `tests/common/cli.rs:73-73` `".rigger"`
- `tests/common/cli.rs:78-78` `".rigger"`
- `tests/common/cli.rs:138-138` `".rigger"`
- `tests/common/cli.rs:149-149` `".rigger"`
- `tests/common/cli.rs:170-170` `".rigger"`
- `tests/common/cli.rs:187-187` `".rigger"`
- `tests/common/cli.rs:317-317` `".rigger"`
- `tests/common/cli.rs:318-318` `"create .rigger/agents"`
- `tests/common/cli.rs:383-383` `"{why}: a server selection must NOT fabricate a local .rigger/events.db"`
- `tests/common/layer_cli.rs:22-22` `".rigger"`
- `tests/common/layer_cli.rs:28-28` `".rigger"`
- `tests/common/mod.rs:281-281` `"{}/.rigger-test-scratch"`
- `tests/common/workflow_probe.rs:15-15` `".rigger"`
- `tests/common/workflow_probe.rs:16-16` `"create .rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:125-125` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:173-173` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:179-179` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:228-228` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:272-272` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:318-318` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:365-365` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:421-421` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:470-470` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:539-539` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:593-593` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:642-642` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:690-690` `".rigger"`
- `tests/config_unknown_key_dotted_path_periphery.rs:740-740` `".rigger"`
- `tests/courier_registry_refresh_boundary_periphery.rs:84-84` `".rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:46-46` `".rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:47-47` `"create .rigger"`
- `tests/courier_registry_refresh_fence_periphery.rs:68-68` `".rigger"`
- `tests/dedup_seeding_periphery.rs:345-345` `".rigger"`
- `tests/dedup_seeding_periphery.rs:372-372` `".rigger"`
- `tests/dedup_seeding_periphery.rs:582-582` `".rigger"`
- `tests/duplication_catalog_contract_periphery.rs:103-103` `".rigger-path string literals"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:121-121` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:157-157` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:278-278` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:568-568` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1108-1108` `".rigger"`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1166-1166` `".rigger"`
- `tests/gate_store_fence_periphery.rs:209-209` `".rigger"`
- `tests/gate_store_fence_periphery.rs:210-210` `".rigger"`
- `tests/gate_store_fence_periphery.rs:215-215` `".rigger"`
- `tests/gate_store_fence_periphery.rs:216-216` `".rigger"`
- `tests/gate_store_fence_periphery.rs:218-218` `".rigger"`
- `tests/gate_store_fence_periphery.rs:223-223` `".rigger"`
- `tests/gate_store_fence_periphery.rs:274-274` `".rigger"`
- `tests/gate_store_fence_periphery.rs:275-275` `".rigger"`
- `tests/gate_store_fence_periphery.rs:282-282` `".rigger"`
- `tests/gate_store_fence_periphery.rs:285-285` `".rigger"`
- `tests/gate_store_fence_periphery.rs:518-518` `".rigger"`
- `tests/gate_store_fence_periphery.rs:541-541` `".rigger"`
- `tests/gate_store_fence_periphery.rs:654-654` `".rigger"`
- `tests/gate_store_fence_periphery.rs:655-655` `".rigger"`
- `tests/gate_store_fence_periphery.rs:665-665` `".rigger"`
- `tests/gate_store_fence_periphery.rs:667-667` `".rigger"`
- `tests/gate_store_fence_periphery.rs:769-769` `".rigger"`
- `tests/gate_store_fence_periphery.rs:770-770` `".rigger"`
- `tests/graph_around_code_first.rs:61-61` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:81-81` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:113-113` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:158-158` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:177-177` `".rigger"`
- `tests/graph_around_governance_boundaries.rs:231-231` `".rigger"`
- `tests/graph_show_surface.rs:81-81` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:596-596` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:621-621` `".rigger"`
- `tests/halted_spawn_wip_recovery_periphery.rs:649-649` `".rigger"`
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
- `tests/heartbeat_write_read_agree_periphery.rs:96-96` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:132-132` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:358-358` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:369-369` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:454-454` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:465-465` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:537-537` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:547-547` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:55-55` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:56-56` `"create .rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:93-93` `".rigger"`
- `tests/init_setup_unknown_key_agent_fleet_periphery.rs:95-95` `"create .rigger/agents"`
- `tests/integrate_conflict_merge_periphery.rs:379-379` `"the project's own .rigger/workflow.yml must load through the real loader"`
- `tests/integrate_conflict_merge_periphery.rs:532-532` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:748-748` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:986-986` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1222-1222` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1463-1463` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1673-1673` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:1865-1865` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2061-2061` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2162-2162` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2329-2329` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2540-2540` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2746-2746` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:2856-2856` `"{repo_path}/.rigger-test-scratch"`
- `tests/integrate_conflict_merge_periphery.rs:3307-3307` `"{repo_path}/.rigger-test-scratch"`
- `tests/land_refused_names_its_paths_periphery.rs:41-41` `"{repo_path}/.rigger-test-scratch"`
- `tests/migration_is_deliberate_periphery.rs:438-438` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:469-469` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:536-536` `".rigger"`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:87-87` `"reclaim_unit_mutation_scratch's own doc comment promises every process rooted in a \
         matched registered mutation-scratch dir is reaped BEFORE the dir is removed - a \
         SIGTERM-ignoring process here must still be SIGKILLed. Round 1 broke this: \
         is_reapable_base's <repo>/.rigger/tmp containment requirement refused this real, \
         ALWAYS-outside-any-repo registered root (see this file's header doc comment and \
         decision u78c2-mutation-scratch-reap-now-refused-flagging-for-review), so \
         reap_processes_rooted_under silently no-opped. Round 2 (decision \
         u78c2r2-authorized-root-caller-supplied) fixed it by passing the registered \
         mutation-scratch root itself as authorized_root - a regression back to the round-1 \
         shape would fail this assertion again."`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:121-121` `".rigger"`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:148-148` `"control: when the registered scratch root legitimately lies under a real repo's \
         .rigger/tmp, reclaim_unit_mutation_scratch must still reap a live process rooted in \
         it - proving the sibling test's failure is specifically the base-guard's new scope, \
         not a defect in this file's own mechanics"`
- `tests/projections_stay_local.rs:106-106` `"the graph projection must be opened by the LOCAL sqlite Projector at .rigger/graph.db \
         (`Projector::open(&db_path(\"graph.db\") ...)`); the canonical local construction is gone"`
- `tests/projections_stay_local.rs:113-113` `"the progress projection must be opened by the LOCAL sqlite Store at .rigger/progress.db \
         (`Store::open(&db_path(\"progress.db\") ...)`); the canonical local construction is gone"`
- `tests/projections_stay_local.rs:200-200` `".rigger"`
- `tests/projections_stay_local.rs:203-203` `"graph.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - projections are per-machine and stay local"`
- `tests/projections_stay_local.rs:213-213` `".rigger"`
- `tests/projections_stay_local.rs:214-214` `"a server-configured `graph build` must NOT create a local .rigger/events.db - the \
             event log is the server's; only the projection is local"`
- `tests/projections_stay_local.rs:267-267` `".rigger"`
- `tests/projections_stay_local.rs:270-270` `"progress.db must be created under the LOCAL .rigger/ even when the event store is the \
             server - the progress store is a local projection, not the shared log"`
- `tests/projections_stay_local.rs:293-293` `".rigger"`
- `tests/projections_stay_local.rs:294-294` `"a server-configured `rigger progress` must NOT create a local .rigger/events.db - the \
             run log is the server's; only the progress projection is local"`
- `tests/published_content_key_split_periphery.rs:258-258` `".rigger"`
- `tests/regate_landed_on_resume_periphery.rs:166-166` `"{repo_path}/.rigger-test-scratch"`
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
- `tests/relocated_worktree_store_resolution_periphery.rs:60-60` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:61-61` `"create .rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:70-70` `".rigger"`
- `tests/relocated_worktree_store_resolution_periphery.rs:132-132` `".rigger"`
- `tests/reset_build_cache_periphery.rs:434-434` `".rigger"`
- `tests/reset_build_cache_periphery.rs:514-514` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1283-1283` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:2981-2981` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:2981-2981` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:2995-2995` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3030-3030` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:47-47` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:48-48` `"create .rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:56-56` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:65-65` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:231-231` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:390-390` `".rigger"`
- `tests/reset_derived_live_writer_guard_periphery.rs:435-435` `"{toplevel}/.rigger/events.db"`
- `tests/reset_derived_live_writer_guard_periphery.rs:469-469` `"/tmp/some-other-project/.rigger/events.db"`
- `tests/reset_derived_live_writer_guard_periphery.rs:514-514` `"/tmp/some-other-project/.rigger/events.db"`
- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:144-144` `"{repo_path}/.rigger-test-scratch"`
- `tests/review_round_no_adjudicator_residue_periphery.rs:93-93` `"{repo_path}/.rigger-test-scratch"`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:119-119` `"{repo_path}/.rigger-test-scratch"`
- `tests/simplification_audit.rs:2828-2828` `".rigger-path string literals"`
- `tests/simplification_audit.rs:2978-2978` `".rigger"`
- `tests/simplification_audit.rs:2979-2979` `"one .rigger-relative path-composition helper"`
- `tests/simplification_audit.rs:4855-4855` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
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
- `tests/simplification_audit.rs:5157-5157` `"#### 10. Consolidate the {rigger_n} `.rigger`-path string-literal sites (`{rigger_id}`) - the \
        single largest cluster in the entire catalog by site count\n\n"`
- `tests/simplification_audit.rs:5161-5161` `"- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
        `proposed_home`) every one of the {rigger_n} sites routes through instead of building its \
        own literal.\n\
        - Files: spans dozens of files including `src/conductor.rs`, `src/config_store.rs`, \
        `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, \
        `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, \
        `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list \
        is in the committed `docs/audit/duplication-catalog.json` under `{rigger_id}` for the \
        follow-up spec to consume directly, not re-enumerated here.\n\
        - Expected line delta: negative - {rigger_n} literal compositions collapse toward one \
        helper's call sites; the helper itself is small.\n\
        - Risk: medium - the largest surface-area sweep in this plan by site count, even \
        though each individual site is trivial; needs a mechanical rewrite pass plus a \
        full-suite green run, not hand-editing {rigger_n} sites.\n\
        - Unblocks: the biggest single site-count reduction available anywhere in the \
        duplication catalog.\n\n"`
- `tests/simplification_audit.rs:8199-8199` `"fn a() {\n    let _ = \".rigger/tmp\";\n}\n"`
- `tests/simplification_audit.rs:8202-8202` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:58-58` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:222-222` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:233-233` `".rigger"`
- `tests/statusline_command_periphery.rs:121-121` `".rigger"`
- `tests/step_sheds_the_freshen.rs:67-67` `".rigger/grounding"`
- `tests/step_sheds_the_freshen.rs:216-216` `".rigger/symbols/index.json"`
- `tests/step_sheds_the_freshen.rs:217-217` `"the surviving persisted index is the SYMBOL index under .rigger/symbols/; got {}"`
- `tests/step_sheds_the_freshen.rs:282-282` `"constructing the default grounder builds and persists the SYMBOL index under \
         .rigger/symbols/ - the freshen's real target"`
- `tests/stop_failure_hook_periphery.rs:56-56` `".rigger/progress.db"`
- `tests/stop_failure_hook_periphery.rs:232-232` `".rigger/events.db"`
- `tests/stop_failure_hook_periphery.rs:247-247` `".rigger/progress.db"`
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
- `tests/store_resolution.rs:214-214` `".rigger"`
- `tests/store_resolution.rs:246-246` `".rigger"`
- `tests/store_resolution.rs:247-247` `"a server-configured courier must NOT create a local .rigger/events.db - that is the \
             state-fracture this criterion closes"`
- `tests/store_resolution.rs:315-315` `".rigger"`
- `tests/store_resolution.rs:351-351` `".rigger"`
- `tests/store_resolution.rs:355-355` `".rigger"`
- `tests/store_resolution_cli.rs:86-86` `"a server-configured courier must NOT create a local .rigger/events.db - that is the \
         state-fracture this criterion closes, and it must hold even when the server is down"`
- `tests/store_secrets.rs:55-55` `".rigger"`
- `tests/store_secrets.rs:105-105` `"{why}: a server-configured courier must NOT create a local .rigger/events.db: {stderr}"`
- `tests/store_secrets.rs:129-129` `".rigger/store.conn secret-file channel"`
- `tests/store_secrets.rs:145-145` `".rigger"`
- `tests/validate_advisories.rs:190-190` `".rigger"`
- `tests/validate_advisories.rs:452-452` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:33-33` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:34-34` `".rigger"`
- `tests/validate_footprint_default_scratch_root_periphery.rs:74-74` `".rigger"`
- `tests/watchdog_cli_periphery.rs:135-135` `".rigger"`
- `tests/watchdog_cli_periphery.rs:173-173` `".rigger"`
- `tests/watchdog_cli_periphery.rs:376-376` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:89-89` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:91-91` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:364-364` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:366-366` `".rigger"`
- `tests/workflow_definition_and_js_constants_periphery.rs:614-614` `".rigger"`
- `tests/workflow_driver_resolved_model_periphery.rs:241-241` `".rigger"`
- `tests/worktree_liveness_fence_periphery.rs:110-110` `".rigger"`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:99-99` `"Worktree::remove's own doc comment promises every process rooted in the worktree is \
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

#### `dup-6bb42578d223` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19647-19667` `render_capped_findings`
- `src/conductor.rs:19889-19908` `render_capped_lessons`
- `src/conductor.rs:22170-22192` `render_capped_lessons_scoped`

#### `dup-b4365447b145` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19670-19737` `findings_prompt_injection_is_capped_under_budget_with_elision_note`
- `src/conductor.rs:22058-22124` `lessons_prompt_injection_is_capped_under_budget_with_elision_note`

#### `dup-759dfad03215` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:21862-21902` `a_subgraph_with_no_design_intent_renders_no_design_intent_header`
- `src/conductor.rs:22025-22055` `a_subgraph_with_no_code_definitions_renders_no_code_neighborhood_header`

#### `dup-f87ed9fec6d6` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:22358-22375` `commit_on_unit_branch`
- `src/conductor.rs:22578-22594` `commit_on_named_branch`

#### `dup-321a8f47e7f4` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:22378-22473` `resume_reuses_a_units_branch_instead_of_reimplementing`
- `src/conductor.rs:24575-24660` `resume_integrates_an_already_approved_unit_without_re_reviewing`
- `src/conductor.rs:24663-24774` `a_failed_unit_is_not_terminal_and_resumes`
- `src/conductor.rs:33958-34061` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted`

#### `dup-7b686b5d0083` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:22476-22570` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume`
- `src/conductor.rs:22609-22665` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch`
- `src/conductor.rs:22887-22979` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first`
- `src/conductor.rs:22982-23063` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers`

#### `dup-b4f4d55cdaa6` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23066-23133` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn`
- `src/conductor.rs:23238-23321` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch`

#### `dup-49e5321ada1e` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23136-23235` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision`
- `src/conductor.rs:23324-23427` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened`

#### `dup-8f8d84022f1f` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:24152-24197` `a_gating_spawn_that_returns_a_reject_verdict_line_is_a_normal_reject_even_if_it_emitted_approve`
- `src/conductor.rs:24200-24244` `a_gating_spawn_with_no_verdict_line_and_no_emitted_approve_is_an_ordinary_reject`

#### `dup-b4bcec70ea0b` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:25099-25141` `scope_creep_refuses_a_criterionless_proposed_unit`
- `src/conductor.rs:30978-31019` `planner_covering_every_criterion_passes`

#### `dup-3d7e127e7901` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:25180-25246` `adversary_runs_between_the_lenses_and_the_adjudicator`
- `src/conductor.rs:25302-25395` `unit_reviews_itself_within_its_own_lifecycle`

#### `dup-abd8e8e23b7d` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:25678-25738` `a_low_risk_unit_skips_the_adversary_and_extra_lens`
- `src/conductor.rs:25741-25811` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing`
- `src/conductor.rs:25832-25910` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it`
- `src/conductor.rs:26025-26089` `a_stage_level_tiers_policy_routes_the_unit_by_risk`

#### `dup-a8bdbc63a1ba` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27745-27807` `planner_proposed_unit_inherits_the_default_review_panel`
- `src/conductor.rs:27810-27905` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents`

#### `dup-39b858fed5e5` (near, 5 sites)

Proposed home: `conductor::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28698-28770` `per_unit_adjudicator_reject_blocks_integration_and_escalates`
- `src/conductor.rs:29298-29371` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage`
- `src/conductor.rs:29467-29533` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage`
- `src/conductor.rs:31661-31708` `on_pass_none_runs_gates_but_does_not_integrate`
- `src/conductor.rs:36534-36579` `unparseable_adjudicator_output_blocks_integration`

#### `dup-b233003304f0` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28869-28966` `a_higher_max_retries_gives_more_attempts_before_escalation`
- `src/conductor.rs:28883-28929` `escalation_run`

#### `dup-ccc12da3ce7c` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29536-29570` `mid_spawn_crash_escalates_without_aborting_the_run`
- `src/conductor.rs:29573-29624` `a_newly_escalated_unit_stamps_an_attention_entry`

#### `dup-f758ca7c9ceb` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29627-29677` `a_budget_halt_stamps_an_attention_entry`
- `src/conductor.rs:30440-30478` `a_budget_halt_surfaces_its_reason_on_the_run_state`

#### `dup-baa6e9166bf9` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29772-29876` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps`
- `src/conductor.rs:29953-30034` `an_escalation_does_not_restamp_attention_on_a_resumed_process`
- `src/conductor.rs:30073-30197` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier`

#### `dup-4fe4c7f9d7a8` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30340-30391` `budget_breaker_stops_the_run_after_the_first_wave`
- `src/conductor.rs:30394-30437` `budget_exhaustion_aborts_the_task`
- `src/conductor.rs:31096-31155` `manual_stage_pauses_while_an_auto_stage_integrates`

#### `dup-32d8fad79719` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30516-30586` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps`
- `src/conductor.rs:30589-30631` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn`

#### `dup-482ea4962db5` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30959-30975` `ungated_fan_out_templates_is_silent_on_a_gated_template`
- `src/conductor.rs:37357-37403` `validate_acyclic_detects_a_cycle`

#### `dup-975bea08853b` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:31158-31201` `isolation_none_agent_gets_no_worktree_even_with_a_repo`
- `src/conductor.rs:31204-31237` `spawn_opts_isolation_is_set_for_a_worktree_agent`
- `src/conductor.rs:31240-31279` `a_spawned_implementers_title_is_the_unit_criterion`

#### `dup-1468d4b7e0dc` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:31286-31334` `the_adversarys_spawn_is_stamped_with_the_units_lens_roster`
- `src/conductor.rs:31340-31389` `the_adjudicators_spawn_is_stamped_with_lenses_plus_adversary`
- `src/conductor.rs:31394-31441` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster`
- `src/conductor.rs:31527-31584` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster`

#### `dup-cd30ba0dffbb` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, tests/postmerge_gate_error_cleanup_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:31719-31734` `spawn`
- `tests/postmerge_gate_error_cleanup_periphery.rs:80-91` `spawn`

#### `dup-df21ef0fa837` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:31738-31825` `two_units_gate_environments_never_share_a_target_dir`
- `src/conductor.rs:31828-31897` `two_units_gate_environments_never_share_a_mutants_root`

#### `dup-717ff53c2a8a` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:32365-32471` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache`
- `src/conductor.rs:32474-32566` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band`
- `src/conductor.rs:32569-32671` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band`

#### `dup-854d13abab44` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:32973-33040` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`
- `src/conductor.rs:34885-34956` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`

#### `dup-58c883e8418c` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:33043-33157` `a_review_rounds_dirty_residue_is_restored_named_and_never_merged`
- `src/conductor.rs:33160-33241` `a_review_rounds_moved_tip_is_restored_to_the_reviewed_sha_before_merging`

#### `dup-67d634e9d610` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:33387-33585` `a_review_rounds_log_derived_start_sha_survives_a_cross_call_resume_after_a_later_tiers_park`
- `src/conductor.rs:33588-33776` `a_review_rounds_log_derived_start_sha_survives_a_same_chunk_sibling_park_across_a_resume`
- `src/conductor.rs:33779-33955` `a_speculation_lanes_log_derived_start_sha_survives_a_cross_call_resume_after_a_park`

#### `dup-86a90e7e30a5` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:35495-35538` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path`
- `src/conductor.rs:35541-35582` `standalone_review_stage_still_takes_the_fan_out_path`

#### `dup-beebc024f3ed` (near, 6 sites)

Proposed home: `conductor::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:35700-35753` `a_parked_lens_keeps_the_standalone_review_stages_worktree`
- `src/conductor.rs:35756-35852` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash`
- `src/conductor.rs:35855-35948` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt`
- `src/conductor.rs:35951-36036` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index`
- `src/conductor.rs:36039-36126` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk`
- `src/conductor.rs:36129-36196` `a_budget_refused_standalone_review_spawn_keeps_its_worktree`

#### `dup-e5b4286f116e` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:36646-36729` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes`
- `src/conductor.rs:36732-36802` `a_product_gate_failure_is_not_rerun_and_demotes_as_before`

#### `dup-7b7b00a8b36d` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:37671-37737` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green`
- `src/conductor.rs:37740-37782` `a_content_change_misses_the_cache_and_re_runs_the_gate`
- `src/conductor.rs:37785-37841` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs`

#### `dup-6f75388894d5` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:37977-38050` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file`
- `src/conductor.rs:38576-38640` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full`

#### `dup-59328344af76` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, tests/revert_on_base_hook_bypass_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:38892-38933` `spawn`
- `src/conductor.rs:39446-39488` `spawn`
- `src/conductor.rs:39942-39979` `spawn`
- `src/conductor.rs:40087-40132` `spawn`
- `tests/revert_on_base_hook_bypass_periphery.rs:78-111` `spawn`

#### `dup-b86205b99ea6` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:39670-39721` `commits_to_compensate_reverts_every_sha_a_multi_commit_landing_recorded`
- `src/conductor.rs:39786-39837` `commits_to_compensate_excludes_the_review_only_marker_even_alongside_a_real_sha`

#### `dup-d2ae28659c98` (near, 2 sites)

Proposed home: `conductor::merge_break_driver`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:40355-40403` `spawn`
- `tests/postmerge_gate_modified_file_periphery.rs:76-120` `spawn`

#### `dup-ca68a744f43b` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:40891-40967` `postmerge_run_gates_err_still_reaps_the_throwaway_worktree_and_branch`
- `src/conductor.rs:40970-41049` `postmerge_worktree_create_err_still_reaps_the_just_created_branch`

#### `dup-eb7cf86024c7` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:41209-41295` `integrate_conflict_re_parks_the_implementer_with_no_attempt_charged_and_both_units_land`
- `src/conductor.rs:41298-41378` `integrate_conflict_confined_to_a_regenerable_path_resolves_with_no_spawn`

#### `dup-bdcc59e0aab8` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:41649-41723` `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline`
- `src/conductor.rs:41726-41790` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done`
- `src/conductor.rs:41828-41899` `a_default_infra_fault_at_a_deferred_gate_does_not_demote`

#### `dup-7c61e67bfa2f` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:41902-42006` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events`
- `src/conductor.rs:42009-42094` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it`

#### `dup-026b4324e7e5` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:43403-43478` `an_empty_accounting_sdet_author_result_advances_the_build_to_commit_gates_and_integrate`
- `src/conductor.rs:43481-43545` `an_absent_sdet_author_is_a_clean_no_op_and_the_build_still_integrates`
- `src/conductor.rs:43548-43623` `a_crashed_sdet_author_spawn_does_not_block_the_build_the_lifecycle_proceeds`

#### `dup-140a34865d26` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, tests/replan_episode_identity.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:43734-43744` `critique_cfg`
- `tests/replan_episode_identity.rs:241-251` `two_episode_cfg`

#### `dup-9a16f7a5d506` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:44534-44632` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated`
- `src/conductor.rs:44809-44880` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review`

#### `dup-2148fe41cedc` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/failure.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:370-372` `is_empty`
- `src/failure.rs:168-170` `is_any`
- `src/main.rs:10568-10573` `is_empty`

#### `dup-e47e931c741d` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, tests/no_os_kill_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1580-1582` `is_word_byte`
- `tests/no_os_kill_audit.rs:59-61` `is_word_char`
- `tests/simplification_audit.rs:202-204` `is_ident_char`

#### `dup-6e5175353084` (near, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:168-188` `read_store_config`
- `src/config_store.rs:225-240` `read_scratch_defaults`

#### `dup-5eaba0e2bfde` (near, 3 sites)

Proposed home: `config_store::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:549-588` `verdict_line_lint_passes_an_unrelated_same_sentence_emit_before_a_verdict_output_clause`
- `src/config_store.rs:600-632` `verdict_line_lint_passes_a_determiner_verdict_when_a_payload_noun_sits_in_the_span`
- `src/config_store.rs:645-685` `verdict_line_lint_passes_a_determiner_verdict_when_an_unrelated_emit_example_brace_precedes_it`

#### `dup-898fea4e5b39` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config_store.rs, src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:721-754` `literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word`
- `src/main.rs:17811-17817` `is_uuid8_accepts_exactly_eight_hex_digits`
- `tests/simplification_audit.rs:8206-8214` `looks_error_shaping_matches_error_and_underscore_bounded_err_but_not_an_incidental_substring`

#### `dup-7da4ecc76203` (near, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:916-923` `parses_agent_frontmatter_and_body`
- `src/config_store.rs:1284-1293` `model_ladder_parses_from_frontmatter`

#### `dup-c10a9e4a46df` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/config_store.rs, src/contextgraph/query.rs, src/main.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:926-928` `rejects_missing_frontmatter`
- `src/contextgraph/query.rs:1936-1938` `graph_load_rejects_malformed_json_without_panicking`
- `src/main.rs:17139-17141` `dirty_tracked_paths_on_a_clean_tree_is_empty`
- `src/spec.rs:949-951` `empty_when_no_criteria`

#### `dup-a8f1cb8eda87` (near, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:1129-1174` `parse_yaml_naming_unknown_keys_recomposes_paths_through_colon_and_marker_bearing_map_keys`
- `src/config_store.rs:1222-1281` `parse_yaml_naming_unknown_keys_escapes_a_literal_dot_inside_a_map_key`

#### `dup-2e7f97275499` (near, 6 sites)

Proposed home: `config_store::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:1374-1383` `defaults_max_wall_clock_parses_and_is_zero_when_absent`
- `src/config_store.rs:1892-1908` `max_retries_parses_from_defaults_and_defaults_to_zero_when_absent`
- `src/config_store.rs:2157-2170` `build_config_parses_wrapper_and_cache_dir_and_defaults_when_omitted`
- `src/config_store.rs:2179-2194` `build_config_parses_jobs_and_defaults_to_zero_when_omitted`
- `src/config_store.rs:2203-2218` `build_config_parses_max_concurrent_defaulting_to_four_when_omitted`
- `src/config_store.rs:2266-2279` `build_config_parses_mutation_and_defaults_to_empty_when_omitted`

#### `dup-e3ad868386ae` (near, 2 sites)

Proposed home: `config_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:1635-1653` `validate_catches_unknown_ref`
- `src/config_store.rs:2471-2499` `validate_catches_cycle`

#### `dup-ae0479d6c9b3` (near, 3 sites)

Proposed home: `config_store::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config_store.rs:1745-1768` `validate_catches_an_unknown_light_panel_agent`
- `src/config_store.rs:1771-1795` `validate_rejects_a_light_panel_with_no_adjudicator`
- `src/config_store.rs:1826-1853` `validate_rejects_a_tiers_policy_on_a_full_panel_with_no_adjudicator`

#### `dup-be97ce907814` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/console/map.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:877-883` `as_candidate`
- `tests/simplification_audit.rs:1815-1824` `map_entry_wire`
- `tests/simplification_audit.rs:1826-1833` `map_entry_lines`

#### `dup-73fd1b3e0b03` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/console/map.rs, tests/files_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:1243-1245` `in_community`
- `tests/files_lens_view_periphery.rs:72-74` `refs`

#### `dup-601e842d257a` (near, 8 sites)

Proposed home: `a new shared module (sites span 3 files: src/console/map.rs, src/ledger.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:1250-1253` `module_of_a_leaf_src_file_is_its_stem_never_the_file_name`
- `src/console/map.rs:1282-1286` `district_purpose_uses_the_curated_table_when_present`
- `src/ledger.rs:878-885` `short_run_id_truncates_to_twelve_chars_and_passes_shorter_ids_through`
- `src/ledger.rs:888-903` `spec_stem_extracts_and_sanitizes_the_file_stem`
- `tests/simplification_audit.rs:7154-7165` `impl_self_type_strips_a_trailing_where_clause_on_a_non_generic_self_type`
- `tests/simplification_audit.rs:7185-7196` `impl_self_type_strips_a_leading_dyn_token_on_the_self_type`
- `tests/simplification_audit.rs:7208-7214` `strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match`
- `tests/simplification_audit.rs:7844-7854` `ident_kind_marker_classifies_by_casing`

#### `dup-fdbf1d3819aa` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/console/map.rs, src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:1265-1279` `module_of_never_returns_a_string_carrying_a_dot_or_a_slash`
- `src/eventstore/mod.rs:794-807` `no_credential_fragment_ever_survives`

#### `dup-0b34706f58ad` (near, 2 sites)

Proposed home: `map::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:1427-1452` `build_breaks_a_dominant_module_tie_by_the_lexicographically_smallest_module`
- `src/console/map.rs:1458-1479` `build_dominant_module_is_chosen_by_true_member_count_not_a_frozen_tie`

#### `dup-7081e6d4e9ca` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/console/map.rs, src/eventstore/mod.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:1952-1967` `budget_scales_the_step_by_zoom_rather_than_offsetting_it`
- `src/eventstore/mod.rs:773-790` `a_delimiter_inside_the_userinfo_never_leaks_the_credential_head`
- `src/spec.rs:1887-1908` `strip_inline_code_direct_exact_output_pins_the_one_span_per_kind_rule`

#### `dup-de9dbebd736c` (near, 2 sites)

Proposed home: `map::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:2101-2115` `caller_callee_graph`
- `src/console/map.rs:2460-2477` `cross_district_graph`

#### `dup-9c2a5feef7eb` (near, 2 sites)

Proposed home: `map::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:2192-2204` `frame_with_no_selection_lights_nothing`
- `src/console/map.rs:2370-2382` `frame_an_unknown_selection_is_a_graceful_no_op`

#### `dup-31f0a377e225` (near, 2 sites)

Proposed home: `map::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:2387-2401` `hit_finds_the_nearest_entity_within_radius`
- `src/console/map.rs:2440-2456` `hit_prefers_an_entity_dot_over_the_district_hull_beneath_it`

#### `dup-0f11f47627d7` (near, 3 sites)

Proposed home: `map::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/map.rs:2480-2487` `landmarks_ranks_by_whole_map_degree_descending`
- `src/console/map.rs:2584-2589` `search_is_case_insensitive`
- `src/console/map.rs:2598-2606` `search_hit_carries_kind_and_degree_beside_the_name`

#### `dup-13cd66603695` (semantic, 2 sites)

Proposed home: `one shared `unix_seconds` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/console/mod.rs:318-322` `unix_seconds`
- `src/dash.rs:2756-2760` `unix_seconds`

#### `dup-dd0899576f8b` (near, 3 sites)

Proposed home: `mod::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/mod.rs:544-558` `dock_lists_a_currently_escalated_unit`
- `src/console/mod.rs:564-575` `dock_lists_a_currently_spent_budget`
- `src/console/mod.rs:640-653` `dock_lists_a_unit_still_failed_past_the_recurrence_threshold`

#### `dup-ebdf0a8c1ad4` (near, 3 sites)

Proposed home: `mod::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/mod.rs:689-702` `statusline_reports_focus_landed_and_working`
- `src/console/mod.rs:707-719` `statusline_reports_done_when_every_unit_landed`
- `src/console/mod.rs:725-735` `statusline_reports_needs_you_over_working`

#### `dup-6e04284d20b9` (near, 2 sites)

Proposed home: `mod::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/console/mod.rs:871-901` `scrub_track_ticks_one_per_hour_boundary_crossed`
- `src/console/mod.rs:913-936` `hour_ticks_never_double_ticks_a_second_event_within_the_seeded_hour`

#### `dup-8120b1fb961a` (near, 2 sites)

Proposed home: `query::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/query.rs:2015-2027` `an_absent_to_endpoint_yields_no_path_even_via_a_dangling_edge`
- `src/contextgraph/query.rs:2036-2047` `neither_endpoint_a_real_node_yields_no_path_even_when_they_are_equal`

#### `dup-e4cc0ac793e5` (semantic, 39 sites)

Proposed home: `one sqlite-connection-opening adapter function every caller is injected with`

mandatory sweep: sqlite Connection::open call sites - 39 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/contextgraph/sqlite.rs:94-94` `Connection::open`
- `src/contextgraph/sqlite.rs:6843-6843` `Connection::open`
- `src/contextgraph/sqlite.rs:7627-7627` `Connection::open`
- `src/eventstore/sqlite.rs:48-48` `Connection::open`
- `src/eventstore/sqlite.rs:1449-1449` `Connection::open`
- `src/eventstore/sqlite.rs:1706-1706` `Connection::open_with_flags`
- `src/eventstore/sqlite.rs:1724-1724` `Connection::open`
- `src/eventstore/sqlite.rs:1738-1738` `Connection::open`
- `src/main.rs:26464-26464` `Connection::open`
- `tests/cli.rs:621-621` `Connection::open`
- `tests/cli.rs:690-690` `Connection::open`
- `tests/cli.rs:786-786` `Connection::open`
- `tests/cli.rs:879-879` `Connection::open`
- `tests/cli.rs:934-934` `Connection::open`
- `tests/cli.rs:10709-10709` `Connection::open`
- `tests/cli.rs:16860-16860` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:69-69` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:165-165` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:189-189` `Connection::open`
- `tests/heartbeat_write_read_agree_periphery.rs:141-141` `Connection::open`
- `tests/reset_derived_compaction.rs:71-71` `Connection::open`
- `tests/reset_derived_compaction.rs:234-234` `Connection::open`
- `tests/reset_derived_compaction.rs:499-499` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:110-110` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:548-548` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:1292-1292` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2548-2548` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2753-2753` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3543-3543` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3634-3634` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3643-3643` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4251-4251` `Connection::open`
- `tests/reset_derived_live_writer_guard_periphery.rs:56-56` `Connection::open`
- `tests/reset_menu.rs:62-62` `Connection::open`
- `tests/reset_menu.rs:66-66` `Connection::open`
- `tests/reset_menu_identity_migration_periphery.rs:97-97` `Connection::open`
- `tests/store_append_order_periphery.rs:58-58` `Connection::open`
- `tests/watchdog_cli_periphery.rs:146-146` `Connection::open`
- `tests/watchdog_cli_periphery.rs:182-182` `Connection::open`

#### `dup-8b23564b85d3` (near, 2 sites)

Proposed home: `sqlite::projector`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:503-618` `calls_down`
- `src/contextgraph/sqlite.rs:654-763` `calls_up`

#### `dup-a88be81e20b2` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2025-2050` `calls_out`
- `src/contextgraph/sqlite.rs:2083-2108` `callers_direct`
- `src/contextgraph/sqlite.rs:2119-2147` `callers_via_bare`

#### `dup-f0a2e1334847` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2864-2877` `subgraph_finds_the_governing_decision`
- `src/contextgraph/sqlite.rs:7984-8004` `recording_proof_never_wipes_the_entitys_own_name_kind_and_line_attrs`

#### `dup-f33f59e9bcbf` (near, 9 sites)

Proposed home: `a new shared module (sites span 3 files: src/contextgraph/sqlite.rs, tests/code_ingest_events.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3281-3288` `apply_edge_inferred`
- `src/contextgraph/sqlite.rs:3322-3329` `apply_edge_inferred_evidence`
- `src/contextgraph/sqlite.rs:4168-4175` `apply_doc_concept`
- `src/contextgraph/sqlite.rs:4278-4285` `apply_doc_link`
- `src/contextgraph/sqlite.rs:4608-4615` `apply_batch_ref`
- `src/contextgraph/sqlite.rs:6186-6196` `apply_unit_integrated`
- `tests/code_ingest_events.rs:891-898` `apply_ref_json`
- `tests/code_ingest_events.rs:1051-1058` `apply_ref_fresh`
- `tests/concepts_fold_periphery.rs:52-59` `realized`

#### `dup-f84eec928003` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3297-3316` `apply_code_entity_partial`
- `src/contextgraph/sqlite.rs:3358-3376` `apply_community`

#### `dup-2d860358c266` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:4288-4411` `design_intent_link_events_fold_into_the_five_design_intent_edges`
- `src/contextgraph/sqlite.rs:4414-4532` `workflow_definition_events_fold_into_stage_gate_agent_nodes_with_needs_runs_reviews_edges`

#### `dup-ef319f80acff` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:4621-4635` `edges_from`
- `src/contextgraph/sqlite.rs:6916-6934` `edges_touching`

#### `dup-f653c159d604` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:5274-5414` `calls_down_walks_the_execution_path_as_a_layered_deduped_dag_with_a_back_edge`
- `src/contextgraph/sqlite.rs:5612-5802` `calls_up_walks_the_call_sites_as_a_layered_deduped_dag_and_lists_referenced_but_not_called`

#### `dup-32d7509fc4e5` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:5919-5952` `decision_fold_projects_no_agent_node_or_decided_edge`
- `src/contextgraph/sqlite.rs:6006-6034` `review_finding_projects_no_raised_edge_even_with_an_event_actor`

#### `dup-7017240c22e7` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6731-6740` `edge_projects`
- `src/contextgraph/sqlite.rs:7792-7804` `index_names`

#### `dup-58ebb5f70eb5` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6743-6825` `every_node_and_edge_carries_the_projects_scope_on_fold`
- `src/contextgraph/sqlite.rs:6937-7028` `prune_is_project_scoped_leaving_another_projects_same_id_node_intact`

#### `dup-25b1ff5f5bfa` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7531-7572` `the_cross_file_inferred_tier_is_order_independent`
- `src/contextgraph/sqlite.rs:7575-7590` `the_definition_upgrade_never_demotes_a_same_file_extracted_reference`

#### `dup-e509388ccd96` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7701-7703` `edge_desc`
- `src/main.rs:8673-8679` `runs_menu_line`

#### `dup-72b8ab279760` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7902-7921` `a_same_file_test_reference_increments_proven_by_and_records_its_evidence`
- `src/contextgraph/sqlite.rs:7924-7947` `two_test_references_accumulate_proven_by_to_2_with_both_evidence_entries`

#### `dup-da0e76ab77ee` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:8026-8057` `an_unresolvable_test_reference_is_staged_and_reconciled_once_its_definition_later_folds`
- `src/contextgraph/sqlite.rs:8302-8333` `the_empty_boundary_sentinel_never_resolves_records_or_stages_anything_for_its_empty_name`

#### `dup-6f592e6f36fa` (semantic, 56 sites)

Proposed home: `src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - see the report's worked example)`

mandatory sweep: /proc-path string literals - 56 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:557-557` `"/proc/net/tcp"`
- `src/dash.rs:588-588` `"/proc"`
- `src/dash.rs:617-617` `"/proc/{pid}/stat"`
- `src/dash.rs:8635-8635` `"/proc"`
- `src/dash.rs:8643-8643` `"the /proc scan must find THIS process as the holder of its own listener"`
- `src/dash.rs:8650-8650` `"/proc"`
- `src/dash.rs:8669-8669` `"/proc"`
- `src/dash.rs:8702-8702` `"/proc"`
- `src/dash.rs:8725-8725` `"/proc"`
- `src/main.rs:15086-15086` `"/proc"`
- `src/main.rs:15190-15190` `"/proc"`
- `src/reap.rs:133-133` `"/proc"`
- `src/reap.rs:215-215` `"/proc/{pid}/stat"`
- `src/reap.rs:226-226` `"/proc/{pid}/status"`
- `src/reap.rs:286-286` `"/proc"`
- `src/reap.rs:363-363` `"/proc/{}/cwd"`
- `tests/claude_code_stream_periphery.rs:1100-1100` `"/proc/{pid}/stat"`
- `tests/cli.rs:22450-22450` `"/proc"`
- `tests/cli.rs:27677-27677` `"/proc"`
- `tests/cli.rs:27762-27762` `"/proc"`
- `tests/cli.rs:27809-27809` `"/proc/{holder_pid}/stat"`
- `tests/cli.rs:27824-27824` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:27890-27890` `"/proc"`
- `tests/cli.rs:27913-27913` `"/proc"`
- `tests/cli.rs:27942-27942` `"/proc"`
- `tests/cli.rs:27992-27992` `"/proc/{holder_pid}/stat"`
- `tests/cli.rs:28007-28007` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:28081-28081` `"/proc"`
- `tests/cli.rs:28107-28107` `"/proc"`
- `tests/cli.rs:28205-28205` `"/proc"`
- `tests/cli.rs:28243-28243` `"held_port_holder's message half and describe_held_port_if_confirmed's own return \
             must agree (scheduler-state letter normalized away since each call independently \
             re-reads /proc and can observe a state flap) - they are documented as sharing one \
             discovery"`
- `tests/cli.rs:28262-28262` `"/proc"`
- `tests/common/fixtures/host.rs:43-43` `"/proc/{pid}/stat"`
- `tests/common/fixtures/host.rs:44-44` `"read /proc/{pid}/stat: {e}"`
- `tests/common/fixtures/host.rs:47-47` `"/proc stat has a parenthesised comm field"`
- `tests/common/fixtures/host.rs:52-52` `"/proc stat has a pgrp field after comm"`
- `tests/duplication_catalog_contract_periphery.rs:101-101` `"/proc-path string literals"`
- `tests/mutation_runner_pdeathsig_periphery.rs:75-75` `"/proc/{pid}/stat"`
- `tests/simplification_audit.rs:2820-2820` `"/proc/<pid>/stat or /proc/<pid>/status field-extraction functions"`
- `tests/simplification_audit.rs:2826-2826` `"/proc-path string literals"`
- `tests/simplification_audit.rs:2966-2966` `"/proc"`
- `tests/simplification_audit.rs:2967-2967` `"src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already \
             duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - \
             see the report's worked example)"`
- `tests/simplification_audit.rs:3007-3007` `"/proc"`
- `tests/simplification_audit.rs:3231-3231` `"src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning \
             whichever field each caller needs, so dash.rs::process_state and \
             reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split"`
- `tests/simplification_audit.rs:3589-3589` `"Two real recall gaps surfaced this way and were closed by widening the mechanical \
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
         (see this subsection's opening paragraph); that exclusion still applies unchanged. A \
         later round's own second commit grew the scanned population from 7513 to 7517 \
         functions, reshuffling the draw; every one of the functions above marked \"no duplicate \
         found by reading\" was re-read by hand against its host file's surrounding context, \
         exactly as this THOROUGHNESS check requires whenever the draw changes, and all are \
         genuinely not duplicates - this redraw surfaced no new recall gap. Two standing shapes \
         an earlier round's reading pass named, neither drawn this time but both still present \
         and still correctly excluded, are restated here so neither is mistaken for a miss on a \
         future draw: `apply` at `src/conductor.rs:38374-38376` \
         (`grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grep_superset`) is a \
         `Projection` test double's own required trait-impl body, the same \
         port-default/adapter-override/test-double shape `find_same_named_helper_functions`'s \
         trait-impl-precision fix (decision `u85c2-same-named-helper-trait-impl-precision-fix`) \
         already excludes from clustering by design; and `gate_verdict_event` \
         (`src/conductor.rs:37733-37742`) together with the `verdict` closure inside \
         `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` \
         (`src/conductor.rs:39138-39147`) do the identical job - find the recorded `GateVerdict` \
         for a `\"<unit>/gate:g#<attempt>\"` replay key, panicking with the same message when none \
         exists - differing only in whether the unit segment is the literal `\"s\"` or a \
         parameter. A `let`-bound closure is not a `fn` item, so no change to this catalog's \
         `fn`-only scanner (module doc, THE SCANNER) short of teaching it to see closures could \
         catalog this pair as a cluster; named here, prominently, rather than silently, so a \
         later refactor - or a scanner that learns to see closures - does not miss it."`
- `tests/simplification_audit.rs:3849-3849` `"A second mutation authority for one domain: the one previously-known \
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
- `tests/simplification_audit.rs:4855-4855` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
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
- `tests/simplification_audit.rs:4969-4969` `"#### 3. Retire the duplicate `/proc`-reading authority (`{}` + `{}`)\n\n"`
- `tests/simplification_audit.rs:4973-4973` `"- Scope: `src/dash.rs::process_state` independently re-derives `/proc/<pid>/stat` \
        fields that `src/reap.rs` (`pid_starttime`/`read_ppid`) already parses - the exact \
        \"second mutation authority\" example spec 85's own Goal names and spec 62's \
        capstone previously caught (`{readers_id}`, {readers_sites} sites across \
        {proc_reader_files}), plus {literal_sites} raw `/proc`-path string literals scattered \
        across {literal_files} files with no shared composer (`{literals_id}`). Both clusters' \
        own `proposed_home` agree: `src/reap.rs` becomes the one `/proc`-reading module; \
        `dash.rs` calls it instead of re-parsing. `process_state` is reachable from `dash`'s \
        own always-on production server, so it is the actual active-correctness risk this \
        tier-1 placement is about; the cluster's test-only readers (such as the shared \
        `tests/common/fixtures/host.rs::pgid_of` fixture) earn no tier-1 placement on their \
        own and ride in this same item only because they share its root cause.\n\
        - Files: `src/dash.rs`, `src/reap.rs`, plus the test-only readers `{readers_id}` \
        names.\n\
        - Expected line delta: negative - retires `process_state`'s own parsing body in favor \
        of calling `reap.rs`'s existing parser.\n\
        - Risk: low. Section 3's own disposition already establishes `process_state` as a \
        duplicate READ-only reimplementation, never a bypassed mutation path - nothing this \
        touches can signal or end a process, so it carries none of the no-os-kill gate's own \
        risk surface; retiring the test-only readers is ordinary test cleanup.\n\
        - Unblocks: retires the codebase's only currently-known live instance of the \
        \"duplicate implementation reconciled after the fact\" pattern the operator's \
        strict-DRY rule targets - the concrete precedent spec 85's own Goal cites.\n\n"`
- `tests/simplification_audit.rs:8084-8084` `"fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n"`
- `tests/simplification_audit.rs:8089-8089` `"fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:8094-8094` `"fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:8185-8185` `"fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n"`
- `tests/simplification_audit.rs:8188-8188` `"/proc"`
- `tests/simplification_audit.rs:8190-8190` `"/proc"`
- `tests/simplification_audit.rs:8254-8254` `"fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n"`

#### `dup-1a85bca8b768` (semantic, 15 sites)

Proposed home: `src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning whichever field each caller needs, so dash.rs::process_state and reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split`

mandatory sweep: /proc/<pid>/stat or /proc/<pid>/status field-extraction functions - 15 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:616-624` `process_state`
- `src/reap.rs:214-221` `pid_starttime`
- `src/reap.rs:225-231` `read_ppid`
- `tests/claude_code_stream_periphery.rs:1099-1107` `is_running`
- `tests/cli.rs:27757-27857` `cmd_dash_gives_the_stopped_listener_diagnosis_naming_resume_or_kill`
- `tests/cli.rs:27936-28051` `step_names_the_stopped_holder_when_the_step_paths_own_auto_start_hits_the_predecessor_scenario`
- `tests/common/fixtures/host.rs:42-55` `pgid_of`
- `tests/mutation_runner_pdeathsig_periphery.rs:74-85` `is_running`
- `tests/simplification_audit.rs:2956-2987` `build_sweep_clusters`
- `tests/simplification_audit.rs:3226-3247` `build_extra_semantic_clusters`
- `tests/simplification_audit.rs:3541-3649` `render_adversarial_sample`
- `tests/simplification_audit.rs:4811-5391` `render_section_6`
- `tests/simplification_audit.rs:8076-8102` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:8180-8191` `proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one`
- `tests/simplification_audit.rs:8248-8261` `proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn`

#### `dup-0c8ae5048676` (semantic, 3 sites)

Proposed home: `dash::response - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:2964-2970` `rendered`
- `src/dash.rs:2971-2977` `text`
- `src/dash.rs:2983-2989` `binary`

#### `dup-c625eace16da` (near, 3 sites)

Proposed home: `dash::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4077-4093` `console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm`
- `src/dash.rs:4321-4335` `console_fonts_route_404s_for_an_unknown_asset`
- `src/dash.rs:5835-5849` `unknown_get_path_is_404`

#### `dup-38a67b4d10f0` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4229-4251` `console_route_never_references_an_external_url`
- `src/dash.rs:4343-4369` `console_page_wires_the_theme_toggles_persistence_round_trip`

#### `dup-f75bb0a20873` (near, 5 sites)

Proposed home: `dash::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4648-4664` `dash_serving_on_is_false_for_a_non_dash_listener`
- `src/dash.rs:4805-4825` `dash_serving_pid_on_reports_the_pid_a_real_dash_response_names`
- `src/dash.rs:4847-4870` `dash_serving_pid_on_assembles_a_head_that_spans_many_read_calls`
- `src/dash.rs:4877-4892` `dash_serving_pid_on_is_none_for_a_non_dash_listener`
- `src/dash.rs:4920-4940` `dash_serving_pid_on_is_none_when_the_pid_header_value_is_not_a_number`

#### `dup-eaae7b5012e1` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4675-4720` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder`
- `src/dash.rs:4743-4794` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block`

#### `dup-3f680da704c9` (near, 6 sites)

Proposed home: `a new shared module (sites span 5 files: src/dash.rs, tests/checkin_mutation_diff_base_periphery.rs, tests/console_palette_periphery.rs, tests/projections_stay_local.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4988-5023` `the_page_layout_cannot_scroll_the_body_horizontally`
- `src/dash.rs:5034-5071` `the_landing_view_lists_instances_and_threads_the_attach_selector`
- `tests/checkin_mutation_diff_base_periphery.rs:105-122` `the_shipped_mutation_gate_guards_on_rigger_run_base_never_a_merge_base`
- `tests/console_palette_periphery.rs:400-415` `the_served_console_page_sends_the_scrub_position_to_palette_commands_when_not_live`
- `tests/projections_stay_local.rs:98-116` `the_graph_and_progress_projections_open_via_the_local_sqlite_constructors`
- `tests/store_resolution.rs:118-134` `the_single_resolver_exists_and_the_old_per_command_helper_is_retired`

#### `dup-bc73ec6a6126` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:8668-8686` `describe_held_port_names_this_process_when_it_holds_the_port_itself`
- `src/dash.rs:8701-8717` `describe_held_port_if_confirmed_names_the_holder_when_independently_confirmed`

#### `dup-4fa082976655` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9109-9111` `layer_of`
- `tests/calls_down_execution_path_periphery.rs:80-82` `layer_of`

#### `dup-4d77d0fdce5a` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9720-9744` `an_absent_explain_leaves_the_graph_route_unchanged`
- `src/dash.rs:9982-10002` `a_cluster_drill_carries_no_memory_field`
- `tests/metadata_card_periphery.rs:349-366` `the_served_route_degrades_gracefully_for_an_unknown_card_subject`

#### `dup-20e69e434836` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10028-10078` `card_graph`
- `tests/metadata_card_periphery.rs:49-100` `fixture_graph`

#### `dup-3be7511516ac` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10192-10205` `card_of_a_file_reports_no_proof_of_its_own`
- `src/dash.rs:10212-10222` `card_tolerates_a_malformed_proof_evidence_attr`

#### `dup-776fd9e153e2` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10360-10402` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one`
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

#### `dup-001b3e86f3c5` (near, 7 sites)

Proposed home: `docs::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:625-704` `render_reset_store_skill`
- `src/docs.rs:709-754` `render_build_graph_skill`
- `src/docs.rs:759-803` `render_reindex_skill`
- `src/docs.rs:808-862` `render_resume_a_run_skill`
- `src/docs.rs:869-919` `render_handle_an_escalation_skill`
- `src/docs.rs:1024-1097` `render_restore_the_dash_skill`
- `src/docs.rs:1106-1176` `render_diagnose_churn_skill`

#### `dup-85412eee5dd7` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/cli.rs:403-430` `persona_is_the_system_prompt_task_is_the_prompt`
- `src/driver/cli.rs:433-444` `recurse_false_drops_the_agent_tool_from_allowed_tools`

#### `dup-1856c00f9b89` (semantic, 2 sites)

Proposed home: `one shared `spawn_request` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/driver/replay.rs:256-277` `spawn_request`
- `tests/common/mod.rs:446-460` `spawn_request`

#### `dup-3631d3538de9` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:382-389` `worker`
- `src/driver/replay.rs:1613-1620` `reviewer`

#### `dup-16f4e615101a` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/driver/replay.rs, tests/claude_code_stream_periphery.rs, tests/common/fixtures/conductor.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:391-398` `opts_for`
- `tests/claude_code_stream_periphery.rs:79-85` `opts`
- `tests/common/fixtures/conductor.rs:18-25` `implementer_opts`

#### `dup-1b30068be20e` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/replay.rs, src/liveness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:534-542` `spawn_scratch_path_is_none_rather_than_relative_for_an_empty_scratch_root`
- `src/liveness.rs:823-831` `marker_path_is_none_rather_than_relative_for_an_empty_scratch_root`

#### `dup-8bac3a592ea8` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:618-631` `reclaim_unit_mutation_scratch_never_cross_matches_a_unit_id_that_is_a_string_prefix_of_another`
- `src/driver/replay.rs:637-648` `reclaim_unit_mutation_scratch_is_a_no_op_for_an_empty_unit_id`

#### `dup-9b530092e109` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/replay.rs, tests/common/fixtures/config.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:1273-1282` `stage`
- `tests/common/fixtures/config.rs:14-20` `agent_with_prompt`

#### `dup-d7ac19769555` (near, 4 sites)

Proposed home: `replay::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:1947-2046` `an_emit_only_approve_gating_persona_hard_errors_on_the_replay_driver`
- `src/driver/replay.rs:2049-2149` `a_concurrent_sibling_approve_does_not_hard_error_a_units_genuine_empty_verdict_reject`
- `src/driver/replay.rs:2152-2251` `a_parked_unanswered_sibling_does_not_suppress_this_units_own_approve_backstop`
- `src/driver/replay.rs:2254-2357` `a_closed_sibling_window_overlapping_this_units_own_approve_still_hard_errors`

#### `dup-03445327d61d` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/workflow.rs, src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/workflow.rs:83-85` `new`
- `src/watch.rs:578-580` `new`

#### `dup-866e5f4c8285` (near, 3 sites)

Proposed home: `contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:172-192` `append_assigns_revisions`
- `src/eventstore/contract.rs:296-341` `backward_stream_read_reverses_set`
- `src/eventstore/contract.rs:345-370` `forward_stream_read_honors_nonzero_from`

#### `dup-a23c56e564d2` (near, 2 sites)

Proposed home: `contract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:248-269` `subscription_replays_then_goes_live`
- `src/eventstore/contract.rs:271-292` `stream_subscription_replays_then_goes_live`

#### `dup-f02a615cda8b` (near, 2 sites)

Proposed home: `kurrentdb::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/kurrentdb.rs:601-606` `a_single_event_reports_the_position_the_server_issued`
- `src/eventstore/kurrentdb.rs:636-646` `a_batch_reports_the_revision_span_the_ack_names`

#### `dup-c5228cf29179` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:154-157` `with_valid_from`
- `src/spawn.rs:573-576` `with_meta`

#### `dup-34ff03908ec6` (semantic, 2 sites)

Proposed home: `mod::appended - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/eventstore/mod.rs:211-215` `all`
- `src/eventstore/mod.rs:219-221` `from_placements`

#### `dup-09b3cd252ed3` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, src/metrics.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:503-509` `factor`
- `src/metrics.rs:494-500` `cost_per_upheld`

#### `dup-705c28b30bbe` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, src/grounder/symbols/events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1022-1027` `direction_sql`
- `src/grounder/symbols/events.rs:739-750` `kind_str`
- `src/grounder/symbols/events.rs:754-763` `lang_str`

#### `dup-93c3800a4ebf` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1491-1528` `measure_derived_duplication_scopes_to_the_stream_prefix`
- `src/eventstore/sqlite.rs:1531-1559` `measure_derived_duplication_on_a_clean_log_reports_no_duplication`
- `src/eventstore/sqlite.rs:1562-1616` `measure_derived_duplication_treats_the_same_key_under_two_covered_types_as_two_distinct_subjects`

#### `dup-bf3c46e0b3c8` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/failure.rs, src/gate.rs, src/ledger.rs, src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/failure.rs:43-49` `as_str`
- `src/gate.rs:92-98` `as_str`
- `src/ledger.rs:47-59` `as_str`
- `src/watch.rs:193-202` `response`

#### `dup-783dbb4d5f73` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/failure.rs, src/gate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/failure.rs:65-67` `reruns`
- `src/failure.rs:74-76` `demotes_on_persistent_failure`
- `src/gate.rs:68-70` `runs_inline`

#### `dup-cd0c403c5416` (near, 3 sites)

Proposed home: `gate::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1177-1213` `exec_runner_exports_cargo_target_dir_only_when_given`
- `src/gate.rs:1216-1244` `exec_runner_forces_cargo_target_dir_onto_build_cache_dir_when_target_dir_is_empty`
- `src/gate.rs:1277-1299` `exec_runner_target_dir_wins_over_build_cache_dir_when_both_are_given`

#### `dup-79a43fc10be7` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1247-1274` `exec_runner_env_vars_reach_the_gate_command_through_the_flock_guard_wrapper`
- `src/gate.rs:1390-1418` `exec_runner_degrades_to_unguarded_when_the_guard_path_cannot_be_opened`

#### `dup-51f28ce4ac01` (near, 4 sites)

Proposed home: `gate::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1587-1606` `build_env_resolves_wrapper_cache_dir_and_incremental_off_when_configured`
- `src/gate.rs:1609-1616` `build_env_derives_the_wrapper_specific_cache_dir_var_name`
- `src/gate.rs:1692-1699` `build_env_jobs_cap_reaches_the_build_when_set`
- `src/gate.rs:1702-1714` `build_env_jobs_cap_is_independent_of_the_wrapper`

#### `dup-89818290a26a` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1635-1676` `exec_runner_applies_the_build_env_it_is_given`
- `src/gate.rs:1733-1752` `exec_runner_applies_the_jobs_cap_it_is_given`

#### `dup-515cc0d4ac39` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1968-1984` `resolve_build_layer_named_wrapper_with_an_uncreatable_dir_errors_naming_dir_and_key`
- `src/gate.rs:2060-2079` `resolve_build_layer_named_wrapper_with_a_preexisting_unwritable_dir_errors_naming_dir_and_key`

#### `dup-7526d45d4438` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/events.rs:20-43` `concept_events`
- `src/grounder/design/events.rs:51-74` `link_events`

#### `dup-0f2f14f8c3ce` (semantic, 3 sites)

Proposed home: `one shared `project_batches` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/design/events.rs:90-114` `project_batches`
- `src/grounder/symbols/events.rs:56-58` `project_batches`
- `src/grounder/workflowdef.rs:245-252` `project_batches`

#### `dup-7664240a3c1c` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/events.rs:200-214` `the_emit_is_deterministic_and_sorts_by_kind_then_id`
- `src/grounder/design/events.rs:320-337` `the_link_emit_is_deterministic_and_sorts_by_rel_then_from_then_to`

#### `dup-d6fb24114b41` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/grounder/design/extract.rs, src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:433-436` `is_markdown`
- `src/spec.rs:541-548` `starts_new_element`
- `tests/simplification_audit.rs:2889-2896` `looks_error_shaping`

#### `dup-6b3780670600` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:532-537` `first_heading`
- `src/grounder/design/extract.rs:540-546` `section_headings`

#### `dup-fe07703e2589` (near, 4 sites)

Proposed home: `extract::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:602-611` `a_load_bearing_decision_doc_becomes_a_single_arch_decision_node`
- `src/grounder/design/extract.rs:614-620` `a_spec_shape_or_loop_discipline_doc_becomes_a_handbook_rule_node`
- `src/grounder/design/extract.rs:623-637` `a_why_comment_in_a_source_file_becomes_a_rationale_node`
- `src/grounder/design/extract.rs:957-966` `a_source_file_is_never_a_usage_doc_and_its_rationale_stays_in_scope`

#### `dup-b2fef22b9e41` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:748-761` `a_rationale_explains_the_file_it_annotates`
- `src/grounder/design/extract.rs:764-780` `a_fenced_code_example_path_is_not_mistaken_for_a_specifies_link`

#### `dup-7e1f522f852f` (near, 2 sites)

Proposed home: `model::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/model.rs:30-37` `node_kind`
- `src/grounder/design/model.rs:81-89` `rel`

#### `dup-fe20f68eb184` (semantic, 2 sites)

Proposed home: `one shared `extract_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/events.rs:171-267` `extract_events`
- `src/grounder/workflowdef.rs:194-224` `extract_events`

#### `dup-6c4197f4d7b0` (semantic, 3 sites)

Proposed home: `src/grounder/symbols/extract.rs::extract as the ONE function that touches source parsing (already its own module doc's claim, architecture 5.5.3) - this file's own scan_file/tokenize are ad hoc scanners for the identical job and should route through an injected-grammar extractor rather than re-deriving structure by hand`

mandatory sweep: bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/extract.rs:34-191` `extract`
- `tests/simplification_audit.rs:211-213` `scan_file`
- `tests/simplification_audit.rs:2208-2314` `tokenize`

#### `dup-83dec5393591` (near, 7 sites)

Proposed home: `extract::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/extract.rs:909-977` `test_annotated_definitions_and_everything_nested_inside_them_are_marked_is_test`
- `src/grounder/symbols/extract.rs:980-1018` `cfg_predicates_naming_test_are_recognized_and_similarly_spelled_tokens_are_not`
- `src/grounder/symbols/extract.rs:1021-1056` `negated_and_cfg_attr_predicates_naming_test_do_not_mark_the_item_test`
- `src/grounder/symbols/extract.rs:1059-1094` `a_not_wrapping_a_non_test_atom_never_marks_the_item_test`
- `src/grounder/symbols/extract.rs:1097-1132` `compound_predicates_with_nested_negation_or_a_non_test_disjunct_do_not_mark_the_item_test`
- `src/grounder/symbols/extract.rs:1135-1175` `a_trailing_same_line_comment_on_a_cfg_test_attribute_does_not_sever_the_scan`
- `src/grounder/symbols/extract.rs:1178-1231` `an_inner_cfg_test_attribute_marks_its_enclosing_module_and_the_module_marks_its_children`

#### `dup-a391c6b864b8` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/extract.rs:1322-1368` `extent_spans_a_destructuring_or_default_brace_signature_to_the_full_body`
- `src/grounder/symbols/extract.rs:1371-1433` `extent_generalizes_across_grammars_python_nested_def_and_js_brace_string`

#### `dup-c1bdf6ee7cd2` (semantic, 2 sites)

Proposed home: `one shared `changed_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/grounder.rs:75-98` `changed_files`
- `src/worktree.rs:611-614` `changed_files`

#### `dup-2aabb2b82325` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:161-163` `commonness_map`
- `src/grounder/symbols/grounder.rs:178-180` `ambiguity_map`

#### `dup-109459b28ca6` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:1070-1081` `a_reference_ranks_below_a_definition_of_the_same_name`
- `src/grounder/symbols/grounder.rs:1322-1336` `ground_ranks_an_exact_name_match_above_a_name_that_merely_contains_the_token`

#### `dup-9432fcaa9237` (near, 2 sites)

Proposed home: `grounder::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:1187-1193` `empty_query_or_zero_k_grounds_nothing`
- `src/grounder/symbols/grounder.rs:1602-1612` `has_strong_match_is_true_for_a_contains_tier_match_of_an_unambiguous_entity`

#### `dup-d45ee6096ba3` (near, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:1239-1285` `a_concurrent_reindex_from_a_second_grounder_is_not_clobbered`
- `src/grounder/symbols/grounder.rs:1288-1314` `reindex_replaces_only_a_changed_files_symbols`
- `src/grounder/symbols/grounder.rs:1615-1642` `reindex_over_a_deleted_file_stops_grounding_it`

#### `dup-5e84e7b4d6c7` (near, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:1713-1780` `scored_hits_a_strictly_higher_tier_always_wins_over_a_worse_commonness_and_lexical`
- `src/grounder/symbols/grounder.rs:1788-1842` `scored_hits_breaks_a_tier_tie_by_strict_rarity_never_by_an_equal_commonness`
- `src/grounder/symbols/grounder.rs:1853-1884` `scored_hits_lexical_never_promotes_a_tied_reference_or_a_tied_second_definition`

#### `dup-f8c8e2e29540` (near, 2 sites)

Proposed home: `mod::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/mod.rs:319-329` `a_file_added_to_the_tree_since_the_index_was_built_is_flagged`
- `src/grounder/symbols/mod.rs:332-347` `a_file_removed_from_the_tree_since_the_index_was_built_is_flagged`

#### `dup-d4aab3ae8e0e` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/registry.rs:105-115` `javascript_tags_query`
- `src/grounder/symbols/registry.rs:126-137` `typescript_tags_query`

#### `dup-071bc310e7eb` (near, 2 sites)

Proposed home: `store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/store.rs:23-28` `index_path`
- `src/grounder/symbols/store.rs:33-38` `lock_path`

#### `dup-664af4b75f14` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/grounder/symbols/store.rs, src/grounder/workflowdef.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/store.rs:255-259` `load_is_none_on_a_cold_start`
- `src/grounder/workflowdef.rs:575-579` `project_events_on_a_missing_workflow_yields_nothing_never_a_crash`

#### `dup-e84b51908c12` (semantic, 2 sites)

Proposed home: `one shared `project_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/workflowdef.rs:232-238` `project_events`
- `tests/common/mod.rs:437-442` `project_events`

#### `dup-a9ddd3832b37` (semantic, 2 sites)

Proposed home: `one shared `install_status_line` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/hooks.rs:137-154` `install_status_line`
- `src/main.rs:13017-13033` `install_status_line`

#### `dup-101d2578e9ab` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:161-172` `installs_and_is_idempotent`
- `src/hooks.rs:186-198` `pretooluse_hook_installs_and_is_idempotent`

#### `dup-b87498bfdd3f` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:175-183` `preserves_other_settings`
- `src/hooks.rs:231-246` `pretooluse_hook_composes_with_the_session_start_hook`

#### `dup-f0e2b5d78ab3` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:263-275` `mcp_server_preserves_other_servers_and_other_top_level_keys`
- `src/hooks.rs:278-287` `mcp_server_self_heals_a_drifted_entry`

#### `dup-fe46e4fc223a` (near, 2 sites)

Proposed home: `hooks::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/hooks.rs:304-321` `status_line_preserves_other_top_level_settings`
- `src/hooks.rs:324-334` `status_line_self_heals_a_drifted_entry`

#### `dup-ca344cae18a6` (near, 2 sites)

Proposed home: `ingest::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ingest.rs:286-288` `graph_index_lag`
- `src/ingest.rs:336-338` `graph_index_lag_sample`

#### `dup-f6d1231a427d` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/ingest.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ingest.rs:450-453` `derived_key_parts`
- `tests/published_content_key_split_periphery.rs:39-42` `split`

#### `dup-1056b386d0e6` (semantic, 2 sites)

Proposed home: `ledger::attention_entry - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/ledger.rs:209-219` `unit_scoped`
- `src/ledger.rs:222-228` `run_scoped`

#### `dup-8d3ce1c32a5c` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:659-672` `marker_filename_hex_escapes_every_byte_outside_alphanumeric_and_hyphen`
- `src/liveness.rs:675-694` `marker_filename_hex_escapes_dots_so_no_encoded_result_can_ever_be_a_path_traversal_component`

#### `dup-c213c764b52f` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/liveness.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:697-712` `marker_filename_is_none_only_for_a_truly_empty_input_so_a_join_can_never_be_a_no_op`
- `src/spec.rs:2152-2154` `heading_level_rejects_more_than_six_hashes`

#### `dup-21dfa9e879a5` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/liveness.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:740-754` `marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do`
- `src/main.rs:21008-21022` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host`

#### `dup-84f9f5d5c24e` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:1198-1211` `any_marker_fresh_finds_a_fresh_marker_nested_under_a_run_id_directory`
- `src/liveness.rs:1214-1228` `any_marker_fresh_is_false_once_every_marker_is_older_than_max_age`

#### `dup-1ad7aae3f918` (near, 4 sites)

Proposed home: `liveness::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:1279-1290` `spawn_is_halted_is_true_once_the_named_spawns_own_marker_has_gone_stale`
- `src/liveness.rs:1293-1303` `spawn_is_halted_is_false_when_the_named_spawns_own_marker_is_still_fresh`
- `src/liveness.rs:1340-1359` `spawn_is_halted_is_false_when_a_sibling_spawn_of_the_same_unit_is_still_live`
- `src/liveness.rs:1362-1373` `spawn_is_halted_ignores_a_live_spawn_belonging_to_a_different_unit`

#### `dup-a493d9ff0ecb` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:1306-1318` `spawn_is_halted_is_false_when_the_named_spawn_already_has_a_real_result`
- `src/liveness.rs:1321-1337` `spawn_is_halted_is_true_when_the_named_spawns_only_result_is_its_own_liveness_fault`

#### `dup-1dede83a534e` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:548-551` `config_rigger_dir`
- `src/main.rs:916-919` `project_identity`
- `src/main.rs:13587-13590` `git_repo`

#### `dup-4841cf90306e` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1672-1674` `usage`
- `src/main.rs:11942-11949` `print_scaffold_pointer`

#### `dup-e110574b66dc` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1785-1787` `find_store_dir_from`
- `tests/simplification_audit.rs:995-997` `scan_target_files`

#### `dup-37fb424fdf8e` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:4757-4816` `cmd_graph_communities`
- `src/main.rs:4836-4895` `cmd_graph_concepts`

#### `dup-e8acd938f6de` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:5442-5455` `read_graph_index_lag`
- `src/main.rs:7313-7332` `dash_read_run`

#### `dup-3dfc1455ab14` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:11090-11109` `dir_size_bytes`
- `tests/reset_build_cache_periphery.rs:56-73` `dir_bytes`

#### `dup-9b1654a7e498` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:12983-12999` `install_lookup_hook`
- `src/main.rs:13017-13033` `install_status_line`
- `src/main.rs:13042-13056` `install_operator_mcp`

#### `dup-1e7392f128ce` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16664-16703` `per_operation_skills_reference_only_real_subcommands`
- `src/main.rs:16713-16741` `watching_discipline_skills_reference_only_real_subcommands`

#### `dup-52233e27e2a1` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17275-17319` `git_is_ancestor_decides_commit_order_in_a_real_repo`
- `src/main.rs:17322-17368` `git_commit_distance_counts_commits_ahead_in_a_real_repo`

#### `dup-fd2b29375126` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17388-17393` `behind_the_tree_message_is_silent_when_versions_already_match`
- `src/main.rs:17396-17406` `behind_the_tree_message_is_silent_when_either_side_is_unversioned`
- `src/main.rs:17409-17418` `behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance`

#### `dup-d129d6d16af4` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17633-17650` `refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them`
- `src/main.rs:17653-17671` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds`
- `src/main.rs:17674-17714` `refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor`

#### `dup-2b6a01e72e97` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18888-18911` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel`
- `src/main.rs:19282-19294` `find_store_dir_from_walks_up_from_a_subdirectory`
- `src/main.rs:19526-19555` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above`

#### `dup-bf2e56140b6c` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, src/reap.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19242-19249` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent`
- `src/reap.rs:853-860` `processes_rooted_under_is_a_graceful_no_op_when_the_base_is_absent`

#### `dup-6ea24d919b4e` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19558-19611` `find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it`
- `src/main.rs:19614-19668` `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store`

#### `dup-ab16c8f475f6` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:20062-20100` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn`
- `src/main.rs:20103-20135` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success`

#### `dup-7cb4d017b035` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:20433-20438` `parse_run_args_rejects_unknown_flags_and_values`
- `src/main.rs:26372-26376` `parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag`

#### `dup-503113873c48` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:21141-21206` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned`
- `src/main.rs:21292-21387` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename`

#### `dup-3493c3ab4888` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22141-22220` `init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently`
- `src/main.rs:22229-22269` `init_project_gitignores_the_store_conn_secret_file_idempotently`

#### `dup-cd539bcae8b6` (near, 4 sites)

Proposed home: `main::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22758-22771` `import_agents_validates_and_rejects_a_malformed_agent`
- `src/main.rs:22780-22802` `import_agents_rejects_an_id_colliding_with_an_existing_agent`
- `src/main.rs:22808-22828` `import_agents_rejects_a_duplicate_id_within_one_import`
- `src/main.rs:22834-22855` `import_agents_rejects_an_agent_with_a_blank_id`

#### `dup-8aae77675253` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23193-23204` `the_step_schema_admits_the_attention_array`
- `src/main.rs:25298-25301` `no_runs_message_points_at_rigger_run`

#### `dup-95358c0dc964` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23977-23994` `format_canary_stats_reports_findings_raised_by_tier`
- `src/main.rs:23999-24009` `format_canary_stats_reports_a_zero_findings_count_honestly`

#### `dup-8090cd14957f` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24015-24021` `format_canary_stats_omits_the_findings_volume_section_when_empty`
- `src/main.rs:24243-24249` `format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one`

#### `dup-47bca8aa31e7` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24159-24178` `format_canary_stats_reports_control_items_and_false_positives`
- `src/main.rs:24186-24201` `format_canary_stats_reports_zero_false_positives_honestly`

#### `dup-6cb9b1500b11` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24458-24503` `stats_discloses_when_no_verdict_was_recorded_on_this_driver`
- `src/main.rs:24514-24587` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed`

#### `dup-b3f279a36f39` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24866-24868` `order_signature_advisories_is_empty_when_no_signatures_are_given`
- `src/main.rs:27668-27670` `parse_guard_write_roots_requires_at_least_one`

#### `dup-8208a30a91e2` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25320-25332` `stats_lines_absent_db_returns_none_and_creates_no_file`
- `src/main.rs:25530-25542` `result_of_at_absent_db_reads_as_unreported_and_creates_no_file`

#### `dup-084a2ca755da` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:25548-25568` `result_of_at_unrecorded_spawn_reads_as_unreported`
- `src/main.rs:25606-25635` `result_of_at_is_namespace_scoped`

#### `dup-2855c9cddfa4` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:26117-26138` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode`
- `src/main.rs:26170-26187` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates`

#### `dup-03eafd1bdc86` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:26704-26797` `implementer_persona_pins_the_checkin_stage_kill_or_justify_contract`
- `src/main.rs:26805-26847` `implementer_persona_pins_the_checkpoint_before_long_work_contract`

#### `dup-06a8cadee157` (near, 5 sites)

Proposed home: `main::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:27081-27089` `grep_guard_decision_denies_every_grep_tool_path`
- `src/main.rs:27109-27125` `grep_guard_decision_denies_a_shell_metacharacter_fused_grep`
- `src/main.rs:27152-27165` `grep_guard_decision_denies_a_redirect_metacharacter_fused_grep`
- `src/main.rs:27188-27201` `grep_guard_decision_denies_a_quoted_or_escaped_grep`
- `src/main.rs:27250-27262` `grep_guard_decision_denies_a_path_qualified_grep`

#### `dup-f1c34d029317` (semantic, 3 sites)

Proposed home: `mcpserver::tool_error - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/mcpserver.rs:36-41` `new`
- `src/mcpserver.rs:45-47` `from`
- `src/mcpserver.rs:51-53` `from`

#### `dup-cef805c01fcd` (semantic, 3 sites)

Proposed home: `one shared `current_run_id` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/mcpserver.rs:532-538` `current_run_id`
- `src/run.rs:163-165` `current_run_id`
- `tests/halted_spawn_wip_recovery_periphery.rs:616-628` `current_run_id`

#### `dup-1e098835f449` (near, 3 sites)

Proposed home: `mcpserver::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1402-1469` `peers_json_core_matches_the_mcp_tool`
- `src/mcpserver.rs:1579-1617` `peers_tool_scopes_to_the_files_arg`
- `src/mcpserver.rs:1620-1665` `peers_tool_surfaces_findings_scoped_to_the_files_arg`

#### `dup-cbfbbbc5102c` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1522-1540` `emit_tool_carries_meta_actor`
- `src/mcpserver.rs:1543-1565` `emit_tool_sets_valid_from_from_nanos`

#### `dup-da04374b96b0` (near, 6 sites)

Proposed home: `mcpserver::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1717-1736` `rigger_result_for_an_unknown_id_is_an_error`
- `src/mcpserver.rs:1739-1761` `malformed_json_gets_a_parse_error`
- `src/mcpserver.rs:1764-1782` `request_missing_method_gets_an_invalid_request_error`
- `src/mcpserver.rs:1785-1803` `tools_call_missing_name_gets_an_invalid_params_error`
- `src/mcpserver.rs:1935-1954` `workflow_surface_rejects_ground_and_graph_as_unknown_tools`
- `src/mcpserver.rs:2194-2212` `spawn_bound_surface_has_no_result_tool`

#### `dup-4903bb384d8c` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1872-1889` `tool_list_with_a_grounder_is_exactly_the_operator_lookup_surface`
- `src/mcpserver.rs:2157-2188` `spawn_bound_tool_list_is_exactly_the_six_named_tools`

#### `dup-fd7d16ecc9d8` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:2218-2243` `spawn_bound_emit_stamps_the_bound_spawn_with_no_meta_supplied`
- `src/mcpserver.rs:2292-2326` `spawn_bound_emit_forces_a_non_object_meta_to_a_stamped_object`

#### `dup-fb8e130faaf9` (exact, 7 sites)

Proposed home: `metrics::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:473-475` `survival`
- `src/metrics.rs:567-569` `lens_overlap_rate`
- `src/metrics.rs:581-583` `first_pass_yield`
- `src/metrics.rs:587-589` `escalation_rate`
- `src/metrics.rs:1210-1212` `rate`
- `src/metrics.rs:1275-1277` `adjudicator_accuracy`
- `src/metrics.rs:1281-1283` `stability_rate`

#### `dup-09df6ba9a4c6` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1446-1448` `changed`
- `src/spawn.rs:579-581` `is_error`

#### `dup-0a954d1e88d1` (exact, 3 sites)

Proposed home: `metrics::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1524-1529` `started`
- `src/metrics.rs:1531-1536` `status`
- `src/metrics.rs:1563-1568` `artifact_verdict`

#### `dup-17538867af44` (near, 6 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1538-1543` `failed`
- `src/metrics.rs:1545-1550` `integrated`
- `src/metrics.rs:1552-1554` `escalated`
- `src/run.rs:358-360` `decision`
- `src/run.rs:361-363` `finding`
- `src/run.rs:364-366` `lesson`

#### `dup-b4de5d907148` (near, 5 sites)

Proposed home: `metrics::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1858-1871` `counts_review_rejects_on_both_per_unit_and_fan_out_paths`
- `src/metrics.rs:1926-1938` `fan_out_reject_then_approve_counts_one_each`
- `src/metrics.rs:1986-1999` `duplicate_unit_started_counts_the_unit_once`
- `src/metrics.rs:2002-2019` `interleaved_units_keep_per_id_review_state`
- `src/metrics.rs:2022-2033` `escalation_is_counted_once_per_unit`

#### `dup-6f8bcb91ed6b` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2090-2101` `finding`
- `src/metrics.rs:2106-2116` `courier_finding`

#### `dup-576605ec7eeb` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2429-2452` `finding_survival_is_upheld_over_raised_per_actor`
- `src/metrics.rs:2524-2552` `cost_per_upheld_finding_is_spawns_over_upheld_per_tier`

#### `dup-3de871af68c4` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2973-2992` `project_canary_counts_correctly_rejected_items_with_empty_attribution`
- `src/metrics.rs:3001-3019` `project_canary_counts_controls_and_false_positives`

#### `dup-7a0ec0f1fd4b` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/progress.rs, tests/reset_derived_compaction.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/progress.rs:330-333` `event_unit_id`
- `tests/reset_derived_compaction.rs:100-103` `replay_key`

#### `dup-7cb3c316417a` (near, 2 sites)

Proposed home: `progress::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/progress.rs:485-492` `spawn_launched_closed_builds_a_closing_record_with_ended_and_class`
- `src/progress.rs:495-500` `spawn_launched_closed_stores_an_empty_class_as_none`

#### `dup-c8ebc048439a` (near, 2 sites)

Proposed home: `progress_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/progress_store.rs:39-53` `record_launch`
- `src/progress_store.rs:59-73` `record_stop_failure`

#### `dup-3439bf494b88` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/reap.rs, tests/common/fixtures/host.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:739-745` `sleeper_in`
- `tests/common/fixtures/host.rs:60-67` `sigterm_ignorer_in`

#### `dup-a6152a39715c` (near, 2 sites)

Proposed home: `reap::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:1296-1322` `signal_if_unchanged_skips_a_starttime_mismatch`
- `src/reap.rs:1325-1352` `signal_if_unchanged_skips_when_cwd_is_outside_the_given_base`

#### `dup-589c05b4ad8a` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/registry.rs, tests/common/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:133-135` `instances_dir`
- `tests/common/cli.rs:72-74` `event_log`
- `tests/common/cli.rs:77-79` `graph_db`

#### `dup-2482a255b7c2` (near, 3 sites)

Proposed home: `registry::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:299-313` `write_then_read_round_trips_a_live_entry`
- `src/registry.rs:411-423` `read_live_no_prune_still_returns_a_fresh_entry`
- `src/registry.rs:426-447` `read_all_returns_a_stale_entry_read_live_would_have_pruned_and_never_deletes_it`

#### `dup-cec0d4808430` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:335-346` `two_projects_get_distinct_entries`
- `src/registry.rs:450-462` `read_all_returns_every_registered_root_regardless_of_freshness`

#### `dup-8640e5c69d2e` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:349-362` `a_reader_prunes_a_stale_heartbeat`
- `src/registry.rs:383-408` `read_live_no_prune_filters_a_stale_heartbeat_but_never_deletes_its_file`

#### `dup-8b14b77d020f` (near, 3 sites)

Proposed home: `run_store::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/run_store.rs:243-309` `the_resolved_base_is_persisted_on_the_run_start_and_survives_adopt`
- `src/run_store.rs:312-383` `the_base_tip_is_persisted_on_the_run_start_and_survives_adopt`
- `src/run_store.rs:386-451` `the_spec_path_is_persisted_on_the_run_start_and_survives_adopt`

#### `dup-c317d66b0546` (near, 2 sites)

Proposed home: `run_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/run_store.rs:474-491` `ensure_started_adopts_the_same_criteria_run_without_re_minting`
- `src/run_store.rs:549-567` `ensure_started_mints_a_new_run_when_the_criteria_change`

#### `dup-3dd926889508` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/sidecar.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:188-190` `len`
- `tests/store_content_identity_periphery.rs:77-79` `batch_calls`

#### `dup-eca5df3ce0b6` (near, 3 sites)

Proposed home: `sidecar::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:238-278` `decisions_for_scopes_to_the_blast_radius`
- `src/sidecar.rs:281-329` `findings_for_scopes_to_the_blast_radius`
- `src/sidecar.rs:332-383` `lessons_for_scopes_to_the_blast_radius`

#### `dup-ea6fa9464303` (near, 2 sites)

Proposed home: `spawn::spawn_result`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:532-539` `ok`
- `src/spawn.rs:544-551` `failed`

#### `dup-6da1db6b6b9c` (semantic, 3 sites)

Proposed home: `spawn::spawn_result - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/spawn.rs:532-539` `ok`
- `src/spawn.rs:544-551` `failed`
- `src/spawn.rs:559-570` `liveness_fault`

#### `dup-fafc632ca1eb` (semantic, 2 sites)

Proposed home: `one shared `liveness_fault` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/spawn.rs:559-570` `liveness_fault`
- `tests/dash_run_tree_spine.rs:81-85` `liveness_fault`

#### `dup-28f66f377bbe` (near, 2 sites)

Proposed home: `spawn_store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn_store.rs:197-231` `record_result_if_absent_is_a_noop_that_never_clobbers_an_existing_result`
- `src/spawn_store.rs:322-359` `record_result_if_absent_honors_a_self_report_that_won_the_race`

#### `dup-1a713042d2fe` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:652-654` `find_word`
- `src/spec.rs:662-664` `find_word_across_hyphen`

#### `dup-349556161f61` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1082-1091` `a_single_coordinator_does_not_flag_multi_behavior`
- `src/spec.rs:1222-1235` `spec_shape_advisories_ignores_coordinators_added_by_continuation_lines`

#### `dup-0d5eef5ac533` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1238-1247` `path_tokens_extracts_relative_file_paths_and_trims_markdown`
- `src/spec.rs:1260-1266` `path_tokens_dedupes_and_preserves_first_seen_order`

#### `dup-800a16d7cea3` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1328-1339` `ownership_check_accepts_the_word_owner`
- `src/spec.rs:1348-1364` `ownership_check_finds_an_owns_sentence_on_a_wrapped_continuation_line`

#### `dup-ef13446197e2` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1383-1407` `ownership_check_finds_an_owns_sentence_on_an_unindented_continuation_line`
- `src/spec.rs:1416-1438` `ownership_check_does_not_reattach_prose_after_a_blank_line_to_the_prior_criterion`

#### `dup-7b967b22273b` (near, 4 sites)

Proposed home: `spec::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1508-1521` `ownership_check_flags_ownerless_and_not_owned_as_denials`
- `src/spec.rs:1531-1550` `ownership_check_does_not_match_owns_or_owner_inside_an_unrelated_word`
- `src/spec.rs:1561-1583` `ownership_check_lets_an_affirmative_owns_win_over_an_unrelated_denial_elsewhere`
- `src/spec.rs:2086-2096` `starts_new_element_recognizes_every_prefix_kind_independently`

#### `dup-315e8a57aba8` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1828-1840` `disposition_check_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `tests/simplification_audit.rs:6767-6771` `a_trait_method_signature_without_a_body_is_not_recorded`

#### `dup-ef9582d62608` (near, 4 sites)

Proposed home: `watch::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:671-677` `a_clean_store_detects_no_anomalies`
- `src/watch.rs:735-749` `two_failures_below_threshold_is_not_reported`
- `src/watch.rs:752-773` `a_cause_change_resets_the_streak_so_three_failures_split_across_two_causes_do_not_alert`
- `src/watch.rs:823-829` `a_spawn_answered_twice_is_below_the_frontier_stall_threshold`

#### `dup-d9507699194d` (near, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:706-732` `a_unit_at_reject_recurrence_three_same_cause_is_reported`
- `src/watch.rs:795-820` `a_spawn_answered_three_times_is_reported_as_a_frontier_stall`

#### `dup-7d44acf98f84` (near, 3 sites)

Proposed home: `watch::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:879-902` `a_fresh_heartbeat_suppresses_the_dead_driver_alert_even_with_a_quiet_store`
- `src/watch.rs:905-933` `a_heartbeat_ten_minutes_stale_does_not_cross_the_thirty_minute_bound`
- `src/watch.rs:936-967` `a_heartbeat_exactly_thirty_minutes_stale_does_not_yet_cross_the_bound`

#### `dup-7be99a62ed80` (near, 3 sites)

Proposed home: `watch::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1026-1049` `a_dash_marker_naming_a_dead_pid_is_reported`
- `src/watch.rs:1059-1087` `a_dead_dash_url_with_no_marker_is_reported_without_inventing_a_pid`
- `src/watch.rs:1159-1187` `dash_attempted_this_run_overrides_a_breadcrumb_that_looks_like_it_predates_the_run`

#### `dup-680698863a6e` (near, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1438-1450` `dedup_suppresses_a_persisting_anomaly_at_the_same_magnitude`
- `src/watch.rs:1469-1482` `dedup_re_alerts_a_cleared_and_later_recurring_anomaly`

#### `dup-fbb2e2648317` (near, 2 sites)

Proposed home: `worktree::worktree`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:635-637` `commit`
- `src/worktree.rs:653-655` `commit_checkpoint`

#### `dup-a29a6c1a667b` (semantic, 3 sites)

Proposed home: `one shared `branch_exists` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/worktree.rs:1443-1445` `branch_exists`
- `tests/postmerge_gate_error_cleanup_periphery.rs:62-70` `branch_exists`
- `tests/step_root_resolution_periphery.rs:182-189` `branch_exists`

#### `dup-ce75d4dcd6eb` (semantic, 2 sites)

Proposed home: `one shared `current_branch` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/worktree.rs:1568-1573` `current_branch`
- `tests/step_root_resolution_periphery.rs:169-180` `current_branch`

#### `dup-17993613795d` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1582-1590` `scratch_root`
- `src/worktree.rs:1681-1689` `scratch_root_path`

#### `dup-cda9c1a42625` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1751-1754` `scratch_root_from_env`
- `src/worktree.rs:1758-1761` `scratch_root_path_from_env`

#### `dup-8a40ee6f30d4` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:3067-3111` `land_reports_untracked_blocking_paths_and_leaves_the_repo_untouched`
- `src/worktree.rs:3114-3148` `land_reports_locally_modified_tracked_blocking_paths`

#### `dup-c8b104ef8278` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:3835-3935` `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_from_a_crash_mid_skip_loop`
- `src/worktree.rs:3938-4047` `cherry_pick_onto_run_branch_self_heals_a_leftover_marker_ahead_of_two_chained_empty_commits`

#### `dup-ab80a443a0c5` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:4853-4880` `changed_files_reports_only_the_rename_destination`
- `src/worktree.rs:6852-6864` `changed_files_unquotes_paths_with_spaces`

#### `dup-952c99c5929f` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5088-5126` `sweep_terminal_removes_merged_worktrees_and_keeps_inflight_ones`
- `src/worktree.rs:5693-5741` `sweep_terminal_reclaims_a_crash_left_terminal_units_per_unit_build_cache`

#### `dup-541e6f1b21c3` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5397-5401` `requested_and_answered`
- `src/worktree.rs:5405-5409` `requested_and_hung`

#### `dup-20eb2c3a9787` (near, 4 sites)

Proposed home: `worktree::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5412-5445` `sweep_terminal_spares_a_merged_branch_whose_units_latest_spawn_is_still_in_flight`
- `src/worktree.rs:5448-5474` `sweep_terminal_reclaims_a_merged_branch_once_its_latest_spawn_has_a_real_result`
- `src/worktree.rs:5477-5501` `sweep_terminal_reclaims_a_merged_branch_whose_latest_spawn_is_hung`
- `src/worktree.rs:5504-5527` `sweep_terminal_reclaims_a_merged_branch_with_no_spawn_recorded_at_all_unchanged`

#### `dup-068178008d84` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5613-5653` `sweep_terminal_prints_evidence_for_a_kept_decision_but_not_for_a_removed_no_spawn_one`
- `src/worktree.rs:5656-5690` `sweep_terminal_prints_evidence_for_a_removed_terminal_spawn_decision`

#### `dup-2d7ee9df221d` (near, 4 sites)

Proposed home: `worktree::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5814-5855` `worktree_remove_reclaims_the_sibling_per_unit_cache`
- `src/worktree.rs:5858-5900` `worktree_remove_also_reclaims_the_sibling_mutants_root`
- `src/worktree.rs:5903-5938` `worktree_remove_also_reclaims_the_store_fence_sibling`
- `src/worktree.rs:5958-5987` `worktree_remove_also_reclaims_a_review_worktrees_store_fence_sibling`

#### `dup-0c5f12097509` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5941-5955` `review_fence_sibling_maps_a_review_worktree_to_its_fence_sibling_and_ignores_the_rest`
- `src/worktree.rs:6269-6282` `unit_cache_sibling_maps_a_unit_worktree_to_its_cache_and_ignores_the_rest`

#### `dup-d728cb89cb95` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:5998-6059` `reclaim_worktree_on_branch_deregisters_the_lingering_worktree_reclaims_its_cache_and_frees_the_branch`
- `src/worktree.rs:6216-6266` `reclaim_worktree_on_branch_prunes_a_stale_registration_whose_dir_was_deleted_and_frees_the_branch`

#### `dup-f88bdbc9c8d0` (near, 3 sites)

Proposed home: `worktree::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:6953-7033` `create_self_heals_a_corrupt_worktree_admin_entry_and_spares_healthy_ones`
- `src/worktree.rs:7036-7110` `create_heals_a_zero_length_gitdir_marker_the_commondir_case_leaves_untested`
- `src/worktree.rs:7113-7179` `create_heals_a_fully_missing_marker_not_just_a_truncated_one`

#### `dup-50e826431b47` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:7182-7218` `heal_never_prunes_a_locked_admin_entry`
- `src/worktree.rs:7221-7246` `heal_never_prunes_an_admin_entry_younger_than_the_grace_period`

#### `dup-74ffd79bb650` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:312-328` `find_unit_started`
- `tests/adoption_keys_on_criterion_periphery.rs:761-780` `find_unit_integrated_commit`

#### `dup-7baafe962b8c` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:789-934` `spec_scoping_blocks_adoption_across_specs_sharing_a_criterion_id_but_not_across_two_runs_of_the_same_spec`
- `tests/adoption_keys_on_criterion_periphery.rs:1601-1741` `a_reused_planner_slug_never_replays_an_unrelated_specs_recorded_adoption_decision`

#### `dup-4cd2de755c71` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1021-1105` `a_compensation_reverted_integration_reopens_adoption_of_its_real_still_existing_branch`
- `tests/adoption_keys_on_criterion_periphery.rs:1113-1183` `a_plain_remediation_failure_after_integration_never_reopens_adoption_even_though_a_same_named_branch_exists`

#### `dup-cac17f66d5aa` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1194-1325` `a_crash_after_the_branch_exists_but_before_unitstarted_lands_recovers_the_recorded_adoption`
- `tests/adoption_keys_on_criterion_periphery.rs:1332-1454` `a_crash_after_the_provenance_record_but_before_the_branch_is_created_still_completes_the_adoption_on_resume`

#### `dup-20256de409e4` (near, 2 sites)

Proposed home: `adoption_keys_on_criterion_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adoption_keys_on_criterion_periphery.rs:1885-2003` `an_escalated_units_unreclaimed_branch_is_never_reused_by_an_unrelated_specs_slug_collision`
- `tests/adoption_keys_on_criterion_periphery.rs:2031-2170` `a_genuine_retry_of_a_quarantined_criterion_adopts_from_the_quarantine_ref`

#### `dup-65c2645daf24` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/build_env_authority_periphery.rs, tests/rigger_run_base_gate_env_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:345-398` `run_once`
- `tests/rigger_run_base_gate_env_periphery.rs:116-167` `run_once`

#### `dup-9b9db533d046` (near, 3 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:433-497` `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess`
- `tests/build_env_authority_periphery.rs:573-625` `jobs_cap_coexists_with_a_configured_wrapper_at_both_real_injection_sites`
- `tests/build_env_authority_periphery.rs:652-698` `auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses`

#### `dup-735f2d3802fc` (near, 2 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:864-939` `run_propagates_a_named_wrappers_uncreatable_cache_dir_at_the_library_entry_point`
- `tests/build_env_authority_periphery.rs:957-1036` `run_propagates_a_named_wrappers_preexisting_unwritable_cache_dir_at_the_library_entry_point`

#### `dup-dd10d8d2efb8` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/build_watch_paths.rs, tests/gitsemver_worktree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_watch_paths.rs:45-52` `fixture_repo`
- `tests/gitsemver_worktree_periphery.rs:61-76` `fixture_repo`

#### `dup-45864f32842e` (semantic, 3 sites)

Proposed home: `one shared `fixture_repo` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_watch_paths.rs:45-52` `fixture_repo`
- `tests/gitsemver_derivation.rs:50-65` `fixture_repo`
- `tests/gitsemver_worktree_periphery.rs:61-76` `fixture_repo`

#### `dup-dc997a3ea272` (near, 2 sites)

Proposed home: `calls_down_execution_path_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/calls_down_execution_path_periphery.rs:257-298` `the_depth_bound_clamps_the_layers_the_walk_returns`
- `tests/calls_down_execution_path_periphery.rs:739-807` `the_up_walk_clamps_the_caller_dag_to_the_depth_bound_and_emits_a_deterministic_layered_order`

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

#### `dup-d97925a56a22` (near, 2 sites)

Proposed home: `canary_model_drift_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:123-176` `canary_and_validate_treat_an_unattributed_tier_as_unmeasured_never_defaulted_from_output_prose`
- `tests/canary_model_drift_periphery.rs:185-265` `canary_and_validate_drift_reads_only_metadata_prose_can_neither_mask_nor_forge_it`

#### `dup-bd13f0d25ce1` (near, 2 sites)

Proposed home: `canary_tolerant_attribution_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_tolerant_attribution_periphery.rs:114-139` `a_tolerant_match_in_a_later_about_entry_still_scores_the_catch`
- `tests/canary_tolerant_attribution_periphery.rs:150-174` `an_empty_about_entry_never_scores_a_catch_even_against_a_trailing_slash_anchor`

#### `dup-f17856d9afa5` (near, 13 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/escalation_resume_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:111-133` `a_legacy_causeless_unit_failed_event_survives_a_real_store_round_trip_and_renders_unknown`
- `tests/cause_wire_periphery.rs:140-172` `the_reject_recurrence_line_names_the_latest_of_several_recorded_causes_through_the_real_binary`
- `tests/cli.rs:10648-10686` `stats_reports_the_latest_run_by_default_and_all_for_the_aggregate`
- `tests/cli.rs:21663-21722` `release_ready_handoff_surfaces_on_status_for_a_done_run`
- `tests/cli.rs:21932-21973` `release_ready_is_silent_on_status_for_an_unfinished_run`
- `tests/cli.rs:22044-22077` `release_ready_pluralizes_the_unit_count_on_status_for_a_multi_unit_run`
- `tests/cli.rs:22122-22144` `release_ready_is_silent_on_status_for_a_spec_defective_run`
- `tests/escalation_resume_periphery.rs:86-110` `a_unit_resumed_event_seeded_directly_through_a_real_store_reaches_status_without_the_command`
- `tests/escalation_resume_periphery.rs:121-150` `a_legacy_shaped_unit_resumed_event_missing_both_optional_fields_survives_a_real_store_round_trip`
- `tests/escalation_resume_periphery.rs:239-270` `the_resumed_banner_survives_a_genuinely_in_flight_re_parked_attempt_not_yet_resolved`
- `tests/escalation_resume_periphery.rs:318-339` `resume_unit_rejects_an_unknown_unit_absent_from_the_run`
- `tests/escalation_resume_periphery.rs:346-370` `resume_unit_refuses_when_the_recorded_branch_was_never_created`
- `tests/escalation_resume_periphery.rs:377-400` `resume_unit_refuses_an_already_integrated_unit`

#### `dup-0744800a9e5c` (near, 2 sites)

Proposed home: `claude_code_stream_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/claude_code_stream_periphery.rs:210-249` `spawn_turns_api_retry_and_the_unparseable_line_into_progress_reports`
- `tests/claude_code_stream_periphery.rs:507-539` `spawn_records_the_mcp_connection_status_from_system_init_as_a_progress_line`

#### `dup-88c4dbe1fddc` (near, 2 sites)

Proposed home: `claude_code_stream_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/claude_code_stream_periphery.rs:252-276` `spawn_errors_loudly_when_the_stream_ends_with_no_result`
- `tests/claude_code_stream_periphery.rs:437-460` `spawn_propagates_a_launch_failure_never_reading_a_stream_that_never_started`

#### `dup-5b90efc91cfa` (near, 4 sites)

Proposed home: `claude_code_stream_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/claude_code_stream_periphery.rs:281-312` `a_child_that_exits_before_init_classifies_unknown_and_carries_the_stderr_tail`
- `tests/claude_code_stream_periphery.rs:315-337` `a_session_with_no_stopfailure_record_classifies_from_the_last_api_retry_category`
- `tests/claude_code_stream_periphery.rs:340-374` `a_stopfailure_record_outranks_the_last_api_retry_category`
- `tests/claude_code_stream_periphery.rs:377-414` `a_stopfailure_record_from_a_different_run_does_not_outrank_the_live_sessions_api_retry`

#### `dup-36cbb5dce011` (near, 2 sites)

Proposed home: `claude_code_stream_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/claude_code_stream_periphery.rs:547-595` `spawn_reaps_the_child_on_a_mid_stream_read_error`
- `tests/claude_code_stream_periphery.rs:654-709` `spawn_survives_a_read_error_that_arrives_after_the_result_line`

#### `dup-a2ee92a6864a` (near, 6 sites)

Proposed home: `claude_code_stream_periphery::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/claude_code_stream_periphery.rs:899-943` `spawn_stop_grace_loop_actually_waits_out_the_injected_duration`
- `tests/claude_code_stream_periphery.rs:946-991` `spawn_escalates_to_the_sanctioned_reap_when_a_silent_child_ignores_its_input_closing`
- `tests/claude_code_stream_periphery.rs:994-1084` `a_concurrent_sibling_spawns_process_in_the_same_worktree_survives_a_wall_clock_stop`
- `tests/claude_code_stream_periphery.rs:1110-1164` `spawn_stop_ends_a_forked_descendant_still_in_the_childs_own_process_tree`
- `tests/claude_code_stream_periphery.rs:1167-1252` `spawn_stop_returns_within_bound_when_a_descendant_has_already_escaped_the_childs_tree`
- `tests/claude_code_stream_periphery.rs:1255-1337` `spawn_returns_a_real_result_promptly_even_when_a_descendant_still_holds_the_stdout_pipe`

#### `dup-e32723f24899` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:201-224` `emit_review_finding_shows_in_peers`
- `tests/cli.rs:8689-8792` `step_surfaces_a_hung_unbounded_spawn_recorded_as_a_liveness_fault_by_the_driver`
- `tests/cli.rs:8817-8880` `step_attention_never_restamps_a_hung_unbounded_spawn_when_repo_less`
- `tests/cli.rs:12127-12191` `a_liveness_fault_on_a_review_spawn_halts_instead_of_re_parking`
- `tests/cli.rs:15519-15549` `result_if_absent_records_when_the_spawn_is_unanswered`
- `tests/cli.rs:15557-15595` `result_if_absent_never_clobbers_a_self_reported_success`

#### `dup-209a533eff7a` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:591-728` `reset_runs_compacts_the_on_disk_graph_after_reclaiming_superseded_rows`
- `tests/cli.rs:758-903` `reset_runs_reports_nonzero_bytes_reclaimed_then_a_second_pass_is_an_idempotent_no_op`

#### `dup-838d5d89040e` (semantic, 2 sites)

Proposed home: `one shared `reported_reclaimed_bytes` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:734-740` `reported_reclaimed_bytes`
- `tests/reset_derived_compaction_periphery.rs:4014-4027` `reported_reclaimed_bytes`

#### `dup-52abb8e62304` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1522-1573` `result_from_a_relocated_git_worktree_outside_the_repo_records_into_the_repo_stream`
- `tests/cli.rs:1586-1628` `result_from_a_configured_nested_git_worktree_records_into_the_repo_stream`

#### `dup-22f21b82b33e` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1779-1850` `a_spawns_scratch_is_reclaimed_the_moment_its_result_is_recorded_for_every_outcome`
- `tests/cli.rs:1866-1940` `a_spawns_mutation_scratch_is_reclaimed_the_moment_its_own_result_reports_for_every_outcome`

#### `dup-ca5b8d6d2708` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1954-1989` `a_reviewers_result_never_reclaims_the_implementers_mutation_scratch`
- `tests/cli.rs:2002-2045` `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots`
- `tests/cli.rs:2062-2115` `a_dotdot_spawn_id_never_escapes_the_pre_existing_agent_scratch_root_either`
- `tests/cli.rs:2136-2174` `a_leading_slash_spawn_id_never_collapses_the_reclaim_to_its_registered_root`

#### `dup-5790877fc434` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2264-2312` `two_speculation_lanes_of_the_same_unit_get_distinct_mutation_scratch_dirs`
- `tests/cli.rs:9559-9623` `a_terminal_units_registered_mutation_scratch_is_reaped_while_a_live_siblings_survives`
- `tests/cli.rs:10389-10506` `a_resumed_run_reaps_an_escalated_and_an_on_pass_none_settled_units_registered_mutation_scratch_not_just_an_integrated_ones`

#### `dup-9d5e6d4b40c2` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2320-2331` `write_grounder_workflow`
- `tests/cli.rs:17237-17252` `write_gating_lint_project`

#### `dup-3a32bd6722e5` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2877-2909` `ground_via_symbols_grounder_ranks_a_definition_first`
- `tests/cli.rs:2988-3017` `ground_via_symbols_grounder_ranks_a_genuinely_rare_contains_tier_entity_above_common_ones_sharing_its_substring`

#### `dup-72906aedbacd` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3484-3540` `worktree_sweep_completes_before_any_add_within_one_step`
- `tests/cli.rs:8898-8933` `the_hung_cursor_is_persisted_only_after_the_step_that_carries_it_is_printed`

#### `dup-3867a053c268` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3663-3710` `step_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:3717-3754` `run_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:3761-3798` `workflow_from_a_linked_worktree_refuses_naming_both_trees`
- `tests/cli.rs:3811-3861` `serve_from_a_linked_worktree_refuses_naming_both_trees`

#### `dup-9da3f693c2b7` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4243-4299` `native_driver_scratch_policy_directs_the_worker_to_its_own_spawn_owned_container`
- `tests/worker_persona_label_periphery.rs:312-333` `workerlabel_is_actually_wired_into_runworkers_agent_call_label`

#### `dup-d29a266a9bb2` (exact, 15 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cli.rs, tests/halted_spawn_wip_recovery_periphery.rs, tests/step_attention_periphery.rs, tests/step_root_resolution_periphery.rs, tests/workflow_driver_resolved_model_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4543-4560` `write_two_stage_workflow`
- `tests/cli.rs:4565-4582` `write_budget_one_two_stage_workflow`
- `tests/cli.rs:5075-5092` `write_reviewless_git_unit_workflow`
- `tests/cli.rs:5099-5117` `write_reviewless_git_escalating_unit_workflow`
- `tests/cli.rs:8012-8030` `write_failing_gate_escalating_workflow`
- `tests/cli.rs:8039-8057` `write_manual_review_workflow`
- `tests/cli.rs:8260-8276` `write_budget_one_dependency_workflow`
- `tests/cli.rs:8343-8349` `write_liveness_workflow`
- `tests/cli.rs:8669-8675` `write_unbounded_liveness_workflow`
- `tests/halted_spawn_wip_recovery_periphery.rs:101-118` `write_solo_unit_workflow`
- `tests/step_attention_periphery.rs:155-173` `write_attention_progression_workflow`
- `tests/step_attention_periphery.rs:376-395` `write_attention_ordering_workflow`
- `tests/step_root_resolution_periphery.rs:146-163` `write_reviewless_git_unit_workflow`
- `tests/workflow_driver_resolved_model_periphery.rs:62-68` `write_one_stage_workflow`
- `tests/worktree_liveness_fence_periphery.rs:127-144` `write_reviewless_git_unit_workflow`

#### `dup-26ae6db7965e` (semantic, 3 sites)

Proposed home: `one shared `write_reviewless_git_unit_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:5075-5092` `write_reviewless_git_unit_workflow`
- `tests/step_root_resolution_periphery.rs:146-163` `write_reviewless_git_unit_workflow`
- `tests/worktree_liveness_fence_periphery.rs:127-144` `write_reviewless_git_unit_workflow`

#### `dup-e905d635ee5c` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5132-5207` `step_reclaims_the_units_worktree_and_deletes_its_branch_on_a_clean_integrate`
- `tests/cli.rs:5222-5290` `step_reclaims_the_units_worktree_but_keeps_its_branch_on_a_terminal_escalation`

#### `dup-45945bd806e6` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5443-5457` `resume_unit_refuses_an_unknown_unit`
- `tests/watchdog_cli_periphery.rs:266-280` `watch_rejects_an_unknown_flag_through_the_real_binary_with_a_nonzero_exit`

#### `dup-35116801ee32` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5529-5738` `step_restores_the_unit_worktree_a_gate_deletes_before_the_review_spawn`
- `tests/cli.rs:5964-6211` `step_stamps_a_real_reviewed_sha_after_repeated_between_step_deletions`
- `tests/cli.rs:6394-6537` `step_stamps_a_real_failed_sha_after_a_deletion_before_the_reject_stamp`

#### `dup-06f248242d71` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:6577-6739` `run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review`
- `tests/cli.rs:7024-7177` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored_end_to_end`
- `tests/cli.rs:9898-10037` `a_speculation_winners_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_winner_integrate_teardown`
- `tests/cli.rs:10051-10189` `a_speculation_escalations_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_escalation_tail_teardown`
- `tests/cli.rs:10212-10362` `a_speculation_on_pass_none_winners_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_on_pass_none_exit_teardown`

#### `dup-485082ea8e22` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:7201-7346` `resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_exhaustive_gates_own_deletion_is_restored`
- `tests/cli.rs:7371-7549` `resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_post_merge_re_gates_own_deletion_is_restored`

#### `dup-9d43338e2f18` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:7858-7919` `run_registers_a_credential_free_shared_instance`
- `tests/cli.rs:7939-8004` `run_driver_workflow_registers_a_credential_free_shared_instance`

#### `dup-b4eb9ad7910c` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8067-8159` `step_carries_the_escalated_set_when_a_fixpoint_is_reached_with_a_wedged_unit`
- `tests/step_attention_periphery.rs:179-278` `recurrence_and_stalled_frontier_survive_real_process_boundaries`

#### `dup-8c5977d41df1` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:9234-9282` `run_teardown_reclaims_run_level_scratch_at_a_definition_drift_halt`
- `tests/cli.rs:9299-9355` `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_hung_spawn_may_be_alive`

#### `dup-5963eaa7c5d4` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:9378-9405` `run_teardown_spares_run_level_scratch_at_a_manual_review_pause`
- `tests/cli.rs:9430-9478` `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_manual_review_is_pending`
- `tests/cli.rs:9488-9536` `run_teardown_reclaims_run_level_scratch_after_a_manual_review_is_integrated`

#### `dup-39f4fabe9676` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:10733-10807` `stats_cli_renders_exact_per_role_spawn_timing_and_unpaired_disclosure`
- `tests/cli.rs:10881-10932` `stats_cli_excludes_suspect_non_positive_duration_pairs_as_unpaired_not_zero`

#### `dup-28168bc8f220` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11283-11311` `step_accepts_base_and_anchors_the_run_branch`
- `tests/cli.rs:11320-11364` `step_creates_run_branch_off_head_when_base_unresolvable`

#### `dup-a3b92ad79a44` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11374-11404` `step_refuses_when_there_is_no_reachable_base`
- `tests/cli.rs:11415-11446` `run_refuses_when_there_is_no_reachable_base`
- `tests/cli.rs:11456-11499` `run_workflow_refuses_when_there_is_no_reachable_base`

#### `dup-1cb17451bc10` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12447-12483` `replay_candidate_column_reacts_to_a_changed_config`
- `tests/cli.rs:12510-12550` `replay_removing_a_gate_lowers_the_candidate_gate_runs`
- `tests/cli.rs:12582-12621` `replay_an_added_gate_fails_safe_and_never_fabricates_a_pass`
- `tests/cli.rs:12651-12683` `replay_an_uncovered_candidate_spawn_parks_and_still_prints_a_partial_column`

#### `dup-a610d4150595` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13009-13033` `validate_fails_at_run_start_when_a_named_build_wrapper_is_absent_from_path`
- `tests/cli.rs:13601-13622` `validate_rejects_an_explicit_build_mutation_value_naming_spec_91_end_to_end`

#### `dup-d8202cc261d7` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13040-13058` `validate_reports_none_when_auto_finds_no_known_wrapper_on_path`
- `tests/cli.rs:13063-13080` `validate_reports_the_resolved_wrapper_when_auto_finds_a_known_wrapper_on_path`
- `tests/cli.rs:13087-13117` `validate_reports_cache_dir_and_budget_alongside_the_wrapper`
- `tests/cli.rs:13268-13293` `validate_reports_none_when_autos_discovered_wrapper_has_an_uncreatable_cache_dir`
- `tests/cli.rs:13339-13365` `validate_reports_none_when_autos_discovered_wrapper_has_a_preexisting_unwritable_cache_dir`
- `tests/cli.rs:13550-13567` `validate_reports_mutation_gate_declared_when_cargo_mutants_is_resolvable`

#### `dup-455c5678334a` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13487-13508` `validate_fails_at_run_start_when_the_scaffolded_mutation_gate_has_no_cargo_mutants_on_path`
- `tests/cli.rs:13525-13545` `validate_fails_before_any_output_when_the_mutation_gate_has_no_cargo_mutants`

#### `dup-60f95b908904` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:14013-14055` `validate_footprint_registered_scratch_roots_measures_the_real_mutation_scratch_root`
- `tests/cli.rs:14431-14541` `validate_footprint_worktrees_and_per_unit_caches_measure_real_dead_and_live_entries_through_the_binary`

#### `dup-38e357732587` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:14077-14203` `validate_flags_registered_scratch_roots_dead_share_scoped_to_real_spawn_liveness_in_the_store`
- `tests/cli.rs:14223-14319` `validate_flags_a_prior_abandoned_runs_orphan_even_when_a_later_run_reuses_the_identical_spawn_id`

#### `dup-23a51d7ab74a` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:15097-15165` `installed_workflow_courier_prompt_is_foreground_and_honest`
- `tests/cli.rs:15185-15263` `installed_workflow_courier_waits_on_an_auto_backgrounded_step`
- `tests/cli.rs:15440-15512` `installed_workflow_driver_guards_a_null_step`

#### `dup-893acdaa7308` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:15802-15869` `step_halts_on_definition_drift_and_rebase_definition_continues`
- `tests/cli.rs:15875-15916` `a_fresh_run_repins_the_current_definition_and_never_halts`

#### `dup-36fd772d194a` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16046-16103` `stats_canary_reports_the_findings_raised_total_summed_across_items`
- `tests/cli.rs:16138-16192` `stats_canary_renders_na_for_a_tier_with_an_unattributed_correct_reject`
- `tests/cli.rs:16203-16252` `stats_canary_still_renders_a_genuine_zero_when_every_reject_has_attribution`
- `tests/cli.rs:16273-16328` `stats_canary_reports_the_control_false_positive_line`
- `tests/cli.rs:16637-16698` `stats_canary_reports_the_model_pinning_header_through_a_real_wire_event`

#### `dup-5c7ae82e0b2e` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/statusline_command_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16108-16118` `stats_canary_on_a_project_with_no_canary_run_says_so`
- `tests/statusline_command_periphery.rs:92-103` `status_line_on_a_clean_run`

#### `dup-10b6b0abe3bc` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16420-16453` `rigger_help_gives_the_jobs_flag_its_own_description_line_through_the_real_binary`
- `tests/cli.rs:16468-16526` `rigger_help_gives_the_model_flag_its_own_description_line_through_the_real_binary`

#### `dup-ad5c04bfa829` (near, 5 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cli.rs, tests/migration_is_deliberate_periphery.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16753-16802` `validate_warns_when_a_tier_resolved_model_repointed_between_runs`
- `tests/cli.rs:16809-16836` `validate_advises_softly_on_a_snapshot_only_date_suffix_bump`
- `tests/cli.rs:16876-16936` `validate_detects_a_stream_whose_position_order_and_revision_order_disagree`
- `tests/migration_is_deliberate_periphery.rs:486-511` `validate_warns_of_retired_code_entities_with_the_measured_count_and_never_fails`
- `tests/validate_advisories.rs:217-242` `validate_warns_of_log_bloat_with_the_measured_factor_and_names_reset_derived`

#### `dup-583e3c262f0a` (semantic, 2 sites)

Proposed home: `one shared `seed_order_signature` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:16848-16869` `seed_order_signature`
- `tests/watchdog_cli_periphery.rs:134-156` `seed_order_signature`

#### `dup-faefc6349bf3` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16942-16962` `canary_if_model_changed_skips_when_the_model_is_unchanged`
- `tests/cli.rs:16969-16991` `canary_if_model_changed_runs_when_a_tier_resolved_model_repointed`
- `tests/cli.rs:17000-17034` `canary_if_model_changed_skips_a_snapshot_only_date_suffix_bump_without_running_the_panel`

#### `dup-ba18e4160c0c` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:17957-18007` `status_reports_not_serving_when_the_recorded_marker_names_a_dead_dash`
- `tests/cli.rs:18020-18065` `status_never_names_the_unattributed_pid_sentinel_as_a_dead_process`

#### `dup-3de39f4663d2` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18076-18135` `status_shows_the_url_when_the_recorded_marker_names_a_genuinely_serving_dash`
- `tests/cli.rs:18149-18216` `status_trusts_a_genuinely_alive_url_even_with_a_mismatched_marker`
- `tests/cli.rs:18232-18310` `status_reports_not_serving_when_a_mismatched_marker_leaves_a_dead_url_unverified`

#### `dup-568e2e76718f` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18647-18709` `docs_ships_graph_hygiene_guidance_to_consumers`
- `tests/cli.rs:18724-18767` `docs_ships_three_verb_lookup_guidance_to_consumers`
- `tests/cli.rs:30418-30474` `docs_installs_the_operator_lookup_rule_text_into_the_shipped_skill_and_handbook`

#### `dup-ea7c7253c23e` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18778-18850` `validate_fails_when_the_committed_using_rigger_docs_drift_and_passes_when_in_sync`
- `tests/cli.rs:18864-18917` `validate_docs_drift_gate_covers_the_second_registry_entry`
- `tests/cli.rs:19014-19068` `validate_docs_drift_gate_covers_the_planning_field_guide_page`

#### `dup-cd82dc5541ee` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19245-19288` `docs_renders_every_per_operation_skill_through_the_compiled_binary`
- `tests/cli.rs:19475-19570` `docs_renders_every_watching_discipline_skill_through_the_compiled_binary`

#### `dup-4dad33ec9f34` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19302-19361` `validate_docs_drift_gate_covers_each_per_operation_skill`
- `tests/cli.rs:19584-19651` `validate_docs_drift_gate_covers_each_watching_discipline_skill`

#### `dup-b27809005a82` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19757-19795` `watch_once_never_names_the_unattributed_pid_sentinel_when_no_url_is_recorded`
- `tests/cli.rs:26505-26543` `watch_once_never_names_the_unattributed_pid_sentinel_when_the_url_is_unparseable`

#### `dup-1e053528cdee` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19815-19848` `watch_once_reports_a_dead_dash_when_only_the_url_breadcrumb_is_recorded_and_no_marker_exists`
- `tests/cli.rs:26565-26594` `watch_once_parses_the_urls_port_past_a_colon_in_the_path_with_no_marker`

#### `dup-a03bc38e5622` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19928-19970` `watch_once_reports_no_dash_anomaly_for_a_done_run_even_with_a_dead_marker`
- `tests/cli.rs:19987-20047` `watch_once_reports_no_dash_anomaly_for_a_fresh_run_that_inherits_an_earlier_runs_dead_marker`
- `tests/cli.rs:20087-20136` `watch_once_reports_this_runs_own_dead_marker_when_written_after_its_run_started`
- `tests/cli.rs:20158-20210` `watch_once_reports_a_dead_marker_predating_run_started_when_dash_attempt_names_this_run`
- `tests/cli.rs:20239-20291` `watch_once_suppresses_a_predating_marker_when_dash_attempt_names_a_different_run`

#### `dup-23d51808a185` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20373-20389` `stage_rigger_shim`
- `tests/cli.rs:20794-20814` `stage_failing_docs_rigger_shim`
- `tests/cli.rs:20842-20870` `stage_stale_rigger_shim`

#### `dup-3f8594b6898c` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20557-20599` `setup_precommit_hook_passes_untouched_when_the_render_matches`
- `tests/cli.rs:20614-20685` `setup_precommit_hook_never_drift_checks_or_stages_a_registry_entry_outside_its_scope`
- `tests/cli.rs:20882-20932` `setup_precommit_hook_prefers_the_trees_own_built_binary_over_a_stale_path_rigger`
- `tests/cli.rs:20942-20992` `setup_precommit_hook_refuses_the_same_commit_shape_with_only_a_stale_path_rigger`
- `tests/cli.rs:21319-21350` `setup_precommit_hook_warns_and_proceeds_when_rigger_is_unavailable`
- `tests/cli.rs:21357-21387` `setup_precommit_hook_warns_and_proceeds_when_rigger_docs_errors`

#### `dup-7dbbb04020b2` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:20821-20832` `stage_tree_built_binary`
- `tests/cli.rs:20999-21014` `stage_unit_derived_binary`

#### `dup-e0fd3ad75f50` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:22497-22542` `step_self_heals_a_stale_marker_naming_a_dead_pid`
- `tests/cli.rs:22567-22613` `step_self_heals_a_stale_marker_naming_a_live_pid_whose_port_is_unserved`

#### `dup-a25efb6375e9` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:23217-23228` `write_live_instance`
- `tests/cli.rs:23234-23245` `write_stale_instance`

#### `dup-54b375032ca2` (near, 7 sites)

Proposed home: `cli::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:23258-23346` `a_reap_on_idle_singleton_serves_while_an_instance_heartbeats_then_reaps_when_the_registry_empties`
- `tests/cli.rs:23462-23546` `a_reap_on_idle_singleton_does_not_reap_before_any_instance_has_registered`
- `tests/cli.rs:23573-23677` `a_reap_on_idle_singleton_survives_a_fresh_agent_liveness_marker_after_the_registry_ages_out`
- `tests/cli.rs:23695-23800` `a_reap_on_idle_singleton_survives_a_fresh_agent_liveness_marker_with_no_git_repo_at_launch`
- `tests/cli.rs:23815-23929` `a_reap_on_idle_singleton_survives_a_second_registered_projects_fresh_agent_liveness_marker`
- `tests/cli.rs:23952-24075` `a_reap_on_idle_singleton_survives_a_foreign_agent_liveness_marker_whose_own_registry_entry_was_already_stale_before_the_watchers_first_poll`
- `tests/cli.rs:24091-24233` `a_landing_poll_racing_the_watchers_first_tick_does_not_erase_a_foreign_projects_only_route_into_known_roots`

#### `dup-a1f3b920fc7c` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:24240-24295` `a_dash_without_reap_on_idle_never_self_reaps_on_a_quiet_machine`
- `tests/cli.rs:24310-24374` `a_reap_on_idle_singleton_in_a_homeless_environment_serves_without_a_watcher`

#### `dup-74b76fba837c` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26157-26199` `watch_once_never_names_a_mismatched_markers_pid_for_the_recorded_urls_port`
- `tests/cli.rs:26623-26672` `watch_once_never_names_a_mismatched_markers_pid_when_the_urls_path_contains_a_colon`

#### `dup-9d43af68ec57` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26811-26849` `run_given_a_spec_path_names_the_spec_lint_as_a_next_step`
- `tests/cli.rs:26974-27017` `step_given_a_spec_path_names_the_spec_lint_even_when_the_step_then_refuses_for_no_reachable_base`
- `tests/cli.rs:27291-27326` `step_reminder_prints_despite_env_naming_a_foreign_pid`
- `tests/cli.rs:27370-27404` `run_reminder_prints_despite_env_naming_a_foreign_pid`

#### `dup-fe895d3c99d5` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27037-27097` `run_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`
- `tests/cli.rs:27106-27145` `step_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`
- `tests/cli.rs:27163-27230` `run_driver_workflow_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`

#### `dup-6bd29f859235` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27250-27286` `step_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid`
- `tests/cli.rs:27330-27365` `run_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid`

#### `dup-42c29bfec243` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27468-27517` `run_driver_workflow_prints_the_spec_lint_reminder_and_honors_the_pid_scoped_dedup`
- `tests/cli.rs:27529-27584` `run_driver_workflow_reminder_never_reaches_stdout_in_any_pid_sentinel_direction`

#### `dup-5da01a770e8a` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27600-27634` `run_driver_workflow_fresh_notice_never_reaches_stdout`
- `tests/cli.rs:27641-27665` `run_driver_cli_fresh_notice_still_prints_on_stdout`

#### `dup-eb93d5a8fdd3` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:27757-27857` `cmd_dash_gives_the_stopped_listener_diagnosis_naming_resume_or_kill`
- `tests/cli.rs:27936-28051` `step_names_the_stopped_holder_when_the_step_paths_own_auto_start_hits_the_predecessor_scenario`

#### `dup-62b22328f7ae` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28402-28509` `mcp_serves_peers_ground_and_graph_over_stdio`
- `tests/cli.rs:30170-30275` `mcp_survives_a_grounder_resolution_failure_and_still_serves_peers_and_graph`

#### `dup-b656a19baf97` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:28941-28968` `run_grep_guard`
- `tests/cli.rs:29063-29093` `run_guard_write`

#### `dup-77feccf69c40` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:29149-29171` `guard_write_resolves_a_relative_target_against_the_payloads_cwd`
- `tests/cli.rs:29200-29221` `guard_write_denies_a_dot_dot_escape_from_inside_the_root`
- `tests/cli.rs:29226-29247` `guard_write_denies_a_symlink_escape`

#### `dup-a69e0d149cd4` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:29708-29737` `grep_guard_denies_an_ancestor_target_end_to_end_and_passes_literal`
- `tests/cli.rs:29780-29794` `grep_guard_still_allows_literal_on_a_shell_metacharacter_fused_grep_end_to_end`
- `tests/cli.rs:29836-29850` `grep_guard_still_allows_a_quoted_literal_on_a_quoted_grep_end_to_end`
- `tests/cli.rs:30040-30066` `grep_guard_bounces_an_output_redirect_metacharacter_fused_grep_end_to_end`

#### `dup-6d0ac21cea6b` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:29746-29771` `grep_guard_bounces_a_shell_metacharacter_fused_grep_end_to_end`
- `tests/cli.rs:29805-29828` `grep_guard_bounces_a_quoted_or_escaped_grep_end_to_end`
- `tests/cli.rs:29897-29919` `grep_guard_bounces_a_path_qualified_grep_end_to_end`
- `tests/cli.rs:29925-29943` `grep_guard_still_allows_literal_on_a_path_qualified_grep_end_to_end`

#### `dup-da0337f50194` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:29857-29873` `grep_guard_bounces_a_grep_split_by_a_line_continuation_end_to_end`
- `tests/cli.rs:29879-29891` `grep_guard_still_allows_literal_on_a_line_continuation_split_grep_end_to_end`

#### `dup-618da8150a9f` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_entity_test_exclusion_periphery.rs, tests/symbol_ref_caller_attribution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:100-139` `is_test_false_serializes_byte_identically_to_the_pre86_form`
- `tests/code_entity_test_exclusion_periphery.rs:266-294` `is_out_of_line_module_false_serializes_byte_identically_to_the_pre_round6_form`
- `tests/code_entity_test_exclusion_periphery.rs:383-411` `path_override_none_serializes_byte_identically_to_the_pre_round7_form`
- `tests/code_entity_test_exclusion_periphery.rs:506-534` `enclosing_inline_module_path_none_serializes_byte_identically_to_the_pre_round9_form`
- `tests/symbol_ref_caller_attribution.rs:32-78` `a_caller_less_reference_serializes_byte_identically_to_the_pre37_form`

#### `dup-44795984d091` (near, 4 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:142-197` `is_test_true_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:297-335` `is_out_of_line_module_true_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:414-453` `path_override_some_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:537-576` `enclosing_inline_module_path_some_serializes_the_key_and_round_trips`

#### `dup-93ee1bbdb003` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_entity_test_exclusion_periphery.rs, tests/symbol_ref_caller_attribution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:200-252` `a_pre86_persisted_index_with_no_is_test_key_loads_defaulting_every_item_to_false`
- `tests/code_entity_test_exclusion_periphery.rs:338-374` `a_pre_round6_persisted_index_with_no_is_out_of_line_module_key_loads_defaulting_to_false`
- `tests/code_entity_test_exclusion_periphery.rs:456-494` `a_pre_round7_persisted_index_with_no_path_override_key_loads_defaulting_to_none`
- `tests/code_entity_test_exclusion_periphery.rs:579-622` `a_pre_round9_persisted_index_with_no_enclosing_inline_module_path_key_loads_defaulting_to_none`
- `tests/symbol_ref_caller_attribution.rs:132-182` `a_pre37_persisted_index_loads_folding_references_caller_less`

#### `dup-cf8ac394deb9` (near, 4 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:759-805` `cfg_not_test_and_cfg_attr_predicates_graph_as_product_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:814-845` `a_not_wrapping_a_non_test_atom_graphs_as_product_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1026-1060` `a_url_bearing_attribute_between_test_and_the_item_does_not_leak_the_item_into_the_graph`
- `tests/code_entity_test_exclusion_periphery.rs:1140-1172` `a_comment_mentioning_test_attribute_text_does_not_exclude_the_item_through_the_public_api`

#### `dup-534a06179667` (near, 26 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 26 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:960-1000` `a_trailing_comment_on_cfg_test_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1085-1121` `a_multiline_cfg_test_attribute_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1210-1252` `a_trailing_comma_in_a_wrapped_cfg_predicate_still_excludes_the_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1280-1322` `an_inner_cfg_test_attribute_excludes_its_module_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1364-1405` `an_out_of_line_cfg_test_module_declaration_excludes_its_declared_file_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1424-1475` `an_out_of_line_cfg_test_module_declaration_in_a_subdirectory_excludes_its_sibling_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1647-1709` `a_non_test_out_of_line_mod_and_an_inline_test_mod_never_exclude_a_coincidentally_named_sibling_file`
- `tests/code_entity_test_exclusion_periphery.rs:1851-1904` `a_cfg_test_impl_block_excludes_its_methods_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1938-1972` `a_cfg_test_impl_block_using_the_inner_attribute_form_excludes_its_methods_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2022-2066` `cfg_test_on_every_other_item_kind_excludes_or_stays_scoped_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2086-2155` `an_out_of_line_test_mod_declared_inside_a_non_directory_style_file_resolves_correctly`
- `tests/code_entity_test_exclusion_periphery.rs:2166-2230` `an_out_of_line_test_mod_declaration_falls_back_to_a_nested_mod_rs_when_no_flat_sibling_exists`
- `tests/code_entity_test_exclusion_periphery.rs:2242-2312` `a_path_attribute_override_redirects_out_of_line_resolution_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2325-2400` `an_out_of_line_test_module_files_own_out_of_line_declarations_are_excluded_recursively_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:2468-2536` `an_out_of_line_test_mod_declaration_falls_back_to_a_root_level_nested_mod_rs_when_module_dir_is_empty`
- `tests/code_entity_test_exclusion_periphery.rs:2549-2618` `a_path_attribute_override_resolves_relative_to_a_declaring_files_own_subdirectory`
- `tests/code_entity_test_exclusion_periphery.rs:2646-2699` `a_path_attribute_override_that_walks_upward_with_dotdot_still_excludes_its_target`
- `tests/code_entity_test_exclusion_periphery.rs:2714-2762` `a_path_attribute_override_with_an_explicit_dot_slash_prefix_still_resolves_to_the_same_directory_target`
- `tests/code_entity_test_exclusion_periphery.rs:2781-2832` `a_path_attribute_override_with_chained_dotdot_walks_up_every_popped_level`
- `tests/code_entity_test_exclusion_periphery.rs:2857-2904` `a_path_attribute_override_whose_dotdot_count_overflows_the_declaring_directorys_depth_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:2930-3007` `a_path_attribute_override_nested_inside_an_inline_module_resolves_under_the_declaring_files_own_module_directory_plus_the_inline_chain`
- `tests/code_entity_test_exclusion_periphery.rs:3028-3126` `a_path_attribute_override_nested_inside_chained_inline_modules_resolves_under_every_enclosing_modules_directory_in_outermost_first_order`
- `tests/code_entity_test_exclusion_periphery.rs:3144-3190` `a_path_attribute_override_that_is_an_absolute_path_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3213-3263` `a_path_attribute_override_naming_a_uri_scheme_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3281-3328` `a_path_attribute_override_naming_a_windows_drive_letter_does_not_silently_collide_with_an_unrelated_real_file`
- `tests/code_entity_test_exclusion_periphery.rs:3363-3421` `a_path_attribute_override_on_a_mod_declared_inside_a_function_body_is_not_treated_as_nested_in_a_module`

#### `dup-1786bc93ea6a` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:234-297` `real_extraction_tiers_every_structural_edge_through_the_emit_fold_pipeline`
- `tests/code_ingest_events.rs:301-371` `real_extraction_folds_caller_attributed_calls_edges_at_every_tier`

#### `dup-e7fabdabc486` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:375-442` `re_extracting_a_file_that_drops_a_call_supersedes_its_calls_edge_end_to_end`
- `tests/code_ingest_events.rs:699-778` `re_extracting_a_changed_file_supersedes_its_removed_symbols_end_to_end`

#### `dup-4fbc57c56116` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:1114-1155` `a_refs_only_files_re_extraction_supersedes_via_the_reference_batch_boundary`
- `tests/code_ingest_events.rs:1158-1212` `a_re_extraction_supersedes_only_its_own_files_edges_not_another_files_reference`

#### `dup-4a87e8dc8806` (near, 2 sites)

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

#### `dup-a28095f8ff23` (semantic, 3 sites)

Proposed home: `one shared `temp_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/common/cli.rs:15-22` `temp_project`
- `tests/published_content_key_split_periphery.rs:255-277` `temp_project`
- `tests/reset_derived_live_writer_guard_periphery.rs:41-51` `temp_project`

#### `dup-8f36b1dc4598` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/cli.rs, tests/courier_registry_refresh_boundary_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:25-29` `temp_project_with_rigger_dir`
- `tests/common/cli.rs:34-38` `courier_project`
- `tests/courier_registry_refresh_boundary_periphery.rs:41-45` `courier_project_with_commit`

#### `dup-20a637ffef9a` (semantic, 2 sites)

Proposed home: `one shared `courier_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/common/cli.rs:34-38` `courier_project`
- `tests/courier_registry_refresh_fence_periphery.rs:39-50` `courier_project`

#### `dup-152df417f690` (semantic, 2 sites)

Proposed home: `one shared `seed_run_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/common/cli.rs:186-200` `seed_run_events`
- `tests/halted_spawn_wip_recovery_periphery.rs:591-609` `seed_run_events`

#### `dup-0dd768841e6e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/cli.rs, tests/reset_derived_compaction.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:203-209` `read_run_events`
- `tests/reset_derived_compaction.rs:546-552` `read_log`

#### `dup-bead66975d97` (semantic, 2 sites)

Proposed home: `one shared `write_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/common/cli.rs:328-336` `write_workflow`
- `tests/common/workflow_probe.rs:21-23` `write_workflow`

#### `dup-d23f3a1d6a5d` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/cli.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/cli.rs:365-385` `assert_selected_server`
- `tests/common/cli.rs:389-405` `assert_selected_sqlite`
- `tests/store_secrets.rs:69-107` `assert_server_reached_and_credentials_redacted`

#### `dup-0b7b830aaf97` (near, 2 sites)

Proposed home: `conductor::noop_driver`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/conductor.rs:195-206` `spawn`
- `tests/scratch_workdir_isolation_leak_guard_periphery.rs:45-56` `spawn`

#### `dup-94b1690dca9c` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/fixtures/config.rs, tests/plan_stage_commit_landing_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/config.rs:103-110` `plan_stage`
- `tests/plan_stage_commit_landing_periphery.rs:428-435` `plan_stage`

#### `dup-c3c8358993cf` (near, 2 sites)

Proposed home: `fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/fold.rs:173-175` `apply_ref`
- `tests/common/fixtures/fold.rs:180-182` `apply_call`

#### `dup-08519818cdf9` (exact, 3 sites)

Proposed home: `git::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/git.rs:92-94` `git_out`
- `tests/common/fixtures/git.rs:97-99` `git_out_with_identity`
- `tests/common/fixtures/git.rs:102-104` `git_stdout`

#### `dup-a3f115da2981` (near, 2 sites)

Proposed home: `graph::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/fixtures/graph.rs:34-43` `entity_node`
- `tests/common/fixtures/graph.rs:152-158` `labelled_node`

#### `dup-cb99d798887c` (exact, 2 sites)

Proposed home: `served::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/served.rs:309-311` `node_harness_passes`
- `tests/common/served.rs:330-332` `run_node_harness`

#### `dup-c99581d3e41e` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/common/workflow_probe.rs, tests/store_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/common/workflow_probe.rs:21-23` `write_workflow`
- `tests/store_precedence.rs:47-49` `write_store_conn`

#### `dup-b02e5a0dd9cd` (semantic, 2 sites)

Proposed home: `one shared `seed_coupling` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_cli.rs:93-130` `seed_coupling`
- `tests/community_fold_periphery.rs:60-70` `seed_coupling`

#### `dup-e64131a21f44` (semantic, 2 sites)

Proposed home: `one shared `community_snapshot` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_pass.rs:33-48` `community_snapshot`
- `tests/community_fold_periphery.rs:40-55` `community_snapshot`

#### `dup-38bcd5cba1f7` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:190-223` `a_community_assigned_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`
- `tests/concepts_fold_periphery.rs:188-232` `a_concept_derived_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`

#### `dup-29092fe5ff01` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:226-260` `the_community_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`
- `tests/concepts_fold_periphery.rs:235-276` `the_concept_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`

#### `dup-515218e72204` (near, 2 sites)

Proposed home: `concepts_derivation_cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_derivation_cli.rs:59-67` `doc`
- `tests/concepts_derivation_cli.rs:72-77` `link`

#### `dup-46bca337e99a` (near, 2 sites)

Proposed home: `concepts_labels_membership::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_labels_membership.rs:103-145` `label_is_the_most_central_document_by_intent_degree`
- `tests/concepts_labels_membership.rs:148-179` `label_ties_break_to_the_lexicographically_smallest_document`

#### `dup-573064ee030c` (near, 2 sites)

Proposed home: `concepts_labels_membership::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_labels_membership.rs:229-256` `a_documentless_concept_falls_back_to_its_most_central_members_name`
- `tests/concepts_labels_membership.rs:259-284` `a_documentless_concept_with_no_named_member_falls_back_to_the_most_central_members_id`

#### `dup-2b83769d8374` (near, 12 sites)

Proposed home: `config_unknown_key_dotted_path_periphery::support (consolidate these 12 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/config_unknown_key_dotted_path_periphery.rs:117-157` `rigger_validate_and_config_store_load_name_the_same_dotted_path_for_an_unknown_key`
- `tests/config_unknown_key_dotted_path_periphery.rs:165-205` `rigger_status_and_read_store_config_name_the_same_dotted_path_for_an_unknown_store_key`
- `tests/config_unknown_key_dotted_path_periphery.rs:220-259` `rigger_validate_passes_through_a_type_mismatch_whose_value_echoes_the_unknown_field_marker`
- `tests/config_unknown_key_dotted_path_periphery.rs:264-299` `rigger_validate_names_the_full_key_when_the_unknown_key_itself_contains_a_backtick`
- `tests/config_unknown_key_dotted_path_periphery.rs:310-346` `rigger_validate_names_the_full_key_when_the_unknown_key_itself_contains_the_terminator_run`
- `tests/config_unknown_key_dotted_path_periphery.rs:357-393` `rigger_validate_recomposes_a_stage_path_through_a_stage_name_containing_its_own_colon_space`
- `tests/config_unknown_key_dotted_path_periphery.rs:413-451` `rigger_validate_names_the_dotted_path_even_when_the_unknown_key_echoes_the_marker_boundary`
- `tests/config_unknown_key_dotted_path_periphery.rs:462-506` `rigger_validate_keeps_the_nested_dotted_path_on_a_genuine_type_mismatch`
- `tests/config_unknown_key_dotted_path_periphery.rs:525-573` `rigger_validate_and_config_store_load_preserve_an_agent_frontmatter_type_mismatch_unchanged`
- `tests/config_unknown_key_dotted_path_periphery.rs:634-671` `rigger_validate_names_the_dotted_path_for_an_unknown_key_inside_a_vec_element`
- `tests/config_unknown_key_dotted_path_periphery.rs:682-718` `rigger_validate_escapes_a_literal_dot_inside_a_stage_name`
- `tests/config_unknown_key_dotted_path_periphery.rs:730-768` `rigger_validate_escapes_a_literal_backslash_inside_a_stage_name`

#### `dup-f99638d3db96` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/console_shell_periphery.rs, tests/dash_console_wasm_route_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/console_shell_periphery.rs:75-107` `the_served_font_route_returns_a_real_woff2_asset_with_correct_binary_headers`
- `tests/dash_console_wasm_route_periphery.rs:63-111` `the_served_console_wasm_route_returns_a_real_wasm_module_with_correct_binary_headers`

#### `dup-2c0773c5730e` (near, 2 sites)

Proposed home: `console_wasm_build_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/console_wasm_build_periphery.rs:198-230` `build_wasm_artifact_scrubs_every_inherited_cargo_feature_env_var_before_spawning`
- `tests/console_wasm_build_periphery.rs:239-277` `build_wasm_artifact_clears_rustflags_and_empties_the_wrapper_vars`

#### `dup-df1f9764f4e9` (semantic, 2 sites)

Proposed home: `one shared `exploration_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_exploration_route_client_contract.rs:67-117` `exploration_graph`
- `tests/dash_kg_graph_route.rs:1131-1179` `exploration_graph`

#### `dup-91477cc75aa4` (near, 2 sites)

Proposed home: `dash_graph_exploration_fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_graph_exploration_fold.rs:37-62` `cluster_key_is_reachable_over_the_public_crate_boundary`
- `tests/dash_graph_exploration_fold.rs:69-146` `cluster_key_honors_the_boundary_edges_of_the_names_a_file_predicate`

#### `dup-89f6632b3d54` (semantic, 3 sites)

Proposed home: `one shared `fixture_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:47-59` `fixture_graph`
- `tests/graph_query_engine_relocation_periphery.rs:62-76` `fixture_graph`
- `tests/metadata_card_periphery.rs:49-100` `fixture_graph`

#### `dup-4a238fd11f57` (near, 2 sites)

Proposed home: `dash_kg_graph_route::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:137-194` `the_served_root_page_ships_the_kg_panel_and_select_to_seed_wiring`
- `tests/dash_kg_graph_route.rs:526-570` `the_served_root_page_renders_god_nodes_and_the_query_path`

#### `dup-fb72166b4d9e` (near, 4 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:520-560` `an_escalated_unit_renders_gates_failed_and_surfaces_at_the_spec_root`
- `tests/dash_run_tree_spine.rs:570-610` `an_escalated_units_gate_failure_is_not_masked_by_a_trailing_passing_gate`
- `tests/dash_run_tree_spine.rs:689-740` `a_regated_green_unit_renders_gates_passed_despite_an_earlier_failed_attempt`
- `tests/dash_run_tree_spine.rs:754-791` `a_review_rejected_unit_whose_gates_passed_renders_gates_passed_and_surfaces_the_reject`

#### `dup-ef040039558b` (near, 2 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:799-829` `a_pre_gate_unit_whose_implementer_finished_does_not_render_gates_failed`
- `tests/dash_run_tree_spine.rs:840-877` `a_gates_cleared_unit_with_no_recorded_verdict_still_renders_gates_passed`

#### `dup-626b09960195` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/duplication_catalog_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:276-294` `every_deserialized_test_only_reference_has_a_non_empty_file_and_content_hash`
- `tests/duplication_catalog_contract_periphery.rs:151-175` `every_deserialized_site_has_a_non_empty_file_name_and_content_hash`

#### `dup-10a4225b14d4` (near, 2 sites)

Proposed home: `dead_code_json_contract_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:439-448` `default_build_config_referenced_only_via_a_serde_default_attribute_is_absent`
- `tests/dead_code_json_contract_periphery.rs:611-623` `dash_marker_parse_the_self_colon_colon_false_positive_stays_absent`

#### `dup-9e4bfd54d15a` (near, 4 sites)

Proposed home: `dead_code_json_contract_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:472-490` `generic_impl_header_constructors_previously_false_flagged_are_absent_from_the_committed_file`
- `tests/dead_code_json_contract_periphery.rs:509-538` `value_position_and_ufcs_reference_shapes_previously_invisible_are_absent_from_the_committed_file`
- `tests/dead_code_json_contract_periphery.rs:551-565` `the_general_ufcs_method_value_fix_also_closes_previously_unreported_same_class_instances`
- `tests/dead_code_json_contract_periphery.rs:581-598` `getter_methods_kept_alive_only_by_a_same_named_production_field_or_local_are_also_absent`

#### `dup-c06e1c41c352` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dead_code_json_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dead_code_json_contract_periphery.rs:645-665` `section_4_3_citations`
- `tests/responsibility_map_contract_periphery.rs:279-300` `section_1_citations`

#### `dup-ff761af86084` (near, 2 sites)

Proposed home: `design_intent_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/design_intent_events.rs:201-247` `the_public_emit_lowers_every_concept_kind_onto_the_fold_arm_that_matches_it`
- `tests/design_intent_events.rs:405-448` `the_public_link_emit_lowers_every_link_rel_onto_the_fold_arm_that_matches_it`

#### `dup-59e92471febe` (near, 2 sites)

Proposed home: `design_intent_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/design_intent_events.rs:788-851` `every_recognized_end_user_usage_shape_is_dropped_before_the_fold`
- `tests/design_intent_events.rs:957-1046` `a_design_word_in_a_non_handbook_usage_doc_does_not_leak_the_handbook_content_keep`

#### `dup-95889ea38145` (near, 3 sites)

Proposed home: `fanout_gate_inheritance_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_gate_inheritance_periphery.rs:99-141` `a_gateless_supersede_of_a_fanout_baseline_still_runs_the_templates_gate_for_real`
- `tests/fanout_gate_inheritance_periphery.rs:147-188` `a_supersede_naming_its_own_gate_unions_it_onto_the_templates_gate_for_real`
- `tests/fanout_gate_inheritance_periphery.rs:197-240` `a_same_id_refine_unions_its_own_gate_onto_the_already_templated_list_for_real`

#### `dup-10da41b33025` (near, 2 sites)

Proposed home: `fanout_template_needs_and_stage_retries_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_template_needs_and_stage_retries_periphery.rs:684-790` `checkin_stays_unready_while_a_real_split_siblings_partner_has_not_integrated_yet`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:799-884` `checkin_never_becomes_ready_when_a_real_split_siblings_partner_escalates_instead`

#### `dup-a56f1137cdfb` (near, 2 sites)

Proposed home: `fanout_template_needs_and_stage_retries_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1103-1152` `a_stages_own_max_retries_yaml_key_lowers_the_effective_bound_below_a_higher_default`
- `tests/fanout_template_needs_and_stage_retries_periphery.rs:1161-1244` `a_stages_own_max_retries_yaml_key_raises_the_effective_bound_above_a_lower_default`

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

#### `dup-1a162aaf091f` (semantic, 2 sites)

Proposed home: `one shared `assert_light_lane_extent_note` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:57-71` `assert_light_lane_extent_note`
- `tests/graph_show_surface.rs:40-49` `assert_light_lane_extent_note`

#### `dup-f83ceb89ff28` (near, 14 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:116-171` `graph_show_degrades_to_stale_note_when_location_drifted`
- `tests/graph_show_periphery.rs:181-223` `graph_show_degrades_to_stale_note_when_recorded_line_is_zero`
- `tests/graph_show_periphery.rs:253-283` `graph_show_light_lane_degrades_to_extent_unavailable_note`
- `tests/graph_show_periphery.rs:292-344` `graph_show_bounds_body_at_the_definitions_own_extent`
- `tests/graph_show_periphery.rs:356-431` `graph_show_shows_full_body_past_nested_definition`
- `tests/graph_show_periphery.rs:442-490` `graph_show_shows_full_body_of_a_destructuring_signature`
- `tests/graph_show_periphery.rs:498-540` `graph_show_extent_ignores_braces_in_strings_comments_and_chars`
- `tests/graph_show_periphery.rs:550-612` `graph_show_shows_full_body_of_a_python_nested_def`
- `tests/graph_show_periphery.rs:623-663` `graph_show_does_not_overread_a_js_single_quote_brace_body`
- `tests/graph_show_periphery.rs:734-787` `graph_show_degrades_when_no_grammar_registered_for_the_file_extension`
- `tests/graph_show_periphery.rs:801-860` `graph_show_heals_to_the_live_line_when_the_moved_name_is_unambiguous`
- `tests/graph_show_periphery.rs:869-921` `graph_show_degrades_when_the_moved_name_is_ambiguous_in_the_file`
- `tests/graph_show_staleness.rs:42-86` `graph_show_degrades_gracefully_when_the_recorded_file_is_missing`
- `tests/graph_show_staleness.rs:103-181` `graph_show_never_presents_a_neighbours_body_when_the_line_drifted`

#### `dup-c579d7abdb85` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:422-499` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1339-1396` `spawn`

#### `dup-72a7cd5d3922` (near, 2 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:921-957` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:1142-1188` `spawn`

#### `dup-09b3e7c0162a` (near, 3 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:1782-1807` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:2259-2281` `spawn`
- `tests/integrate_conflict_merge_periphery.rs:2504-2524` `spawn`

#### `dup-d7cc094ada47` (near, 4 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2047-2144` `a_crash_right_after_the_merge_attempt_record_resumes_and_completes_row_1`
- `tests/integrate_conflict_merge_periphery.rs:2148-2245` `a_crash_right_after_the_landing_intent_record_resumes_and_completes_row_4`
- `tests/integrate_conflict_merge_periphery.rs:2732-2832` `a_crash_right_after_the_merge_succeeds_resumes_and_completes_row_1_after_record`
- `tests/integrate_conflict_merge_periphery.rs:2842-2940` `a_crash_right_after_landing_succeeds_resumes_and_completes_row_4_after_record`

#### `dup-676391cb986b` (near, 3 sites)

Proposed home: `integrate_conflict_merge_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/integrate_conflict_merge_periphery.rs:2561-2721` `a_confined_regenerate_command_failure_and_a_store_failure_each_resume_and_complete_row_3`
- `tests/integrate_conflict_merge_periphery.rs:3003-3105` `a_regenerate_command_failure_right_after_landing_completes_row_3_on_resume_when_row_4_is_already_closed`
- `tests/integrate_conflict_merge_periphery.rs:3118-3238` `a_crash_right_after_landing_succeeds_with_owed_regeneration_completes_row_3_on_resume`

#### `dup-8ea961567395` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:160-179` `no_source_still_gates_on_the_retired_kurrentdb_feature`
- `tests/turbovec_retired.rs:114-132` `no_source_still_gates_on_the_retired_turbovec_feature`

#### `dup-238f6df687da` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/metadata_card_handoff_viz.rs:101-116` `build_harness`
- `tests/proof_row_renders_on_the_card.rs:53-68` `build_harness`
- `tests/subject_lens_overlay_client_arms.rs:60-75` `build_harness`
- `tests/subject_view_memory_rail_client.rs:78-93` `build_harness`

#### `dup-0c9c3cbd4b5a` (semantic, 4 sites)

Proposed home: `one shared `build_harness` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/metadata_card_handoff_viz.rs:101-116` `build_harness`
- `tests/proof_row_renders_on_the_card.rs:53-68` `build_harness`
- `tests/subject_lens_overlay_client_arms.rs:60-75` `build_harness`
- `tests/subject_view_memory_rail_client.rs:78-93` `build_harness`

#### `dup-a1bb5f4f31db` (near, 2 sites)

Proposed home: `migration_is_deliberate_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/migration_is_deliberate_periphery.rs:432-459` `seed_a_retired_entity`
- `tests/migration_is_deliberate_periphery.rs:463-483` `seed_a_live_entity`

#### `dup-2be2113adc3a` (near, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:197-217` `shape_arg_dashdash`
- `tests/no_os_kill_audit.rs:221-241` `shape_format_dash_brace`

#### `dup-a76bc2af4d4b` (near, 11 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 11 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:361-375` `command_new_shell_out_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:378-385` `bare_shell_kill_dash_form_is_caught`
- `tests/no_os_kill_audit.rs:388-395` `standalone_pkill_token_is_caught`
- `tests/no_os_kill_audit.rs:398-405` `pg_signal_call_name_is_caught`
- `tests/no_os_kill_audit.rs:408-415` `libc_kill_call_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:418-425` `signal_kill_call_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:428-441` `kill_process_call_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:466-480` `a_finding_names_its_exact_file_and_line_number`
- `tests/no_os_kill_audit.rs:483-500` `kill_process_is_never_flagged_inside_either_sanctioned_file`
- `tests/no_os_kill_audit.rs:503-517` `a_shell_out_inside_a_sanctioned_file_is_still_caught`
- `tests/no_os_kill_audit.rs:550-563` `a_shape_outside_src_and_tests_is_never_scanned`

#### `dup-7f174d1fe10b` (near, 4 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:444-452` `arg_dashdash_separator_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:455-463` `negative_pid_format_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:520-532` `a_dashdash_separator_inside_a_sanctioned_file_is_still_caught`
- `tests/no_os_kill_audit.rs:535-547` `a_negative_pid_format_inside_a_sanctioned_file_is_still_caught`

#### `dup-3e64f9e11fa8` (near, 2 sites)

Proposed home: `parallel_ordered_emit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/parallel_ordered_emit.rs:71-79` `drive_default`
- `tests/parallel_ordered_emit.rs:82-93` `drive_paced`

#### `dup-e4f67a924cc1` (near, 3 sites)

Proposed home: `phase_of_role_mapping_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/phase_of_role_mapping_periphery.rs:87-102` `plan_and_plan_critique_resolve_to_plan_regardless_of_role`
- `tests/phase_of_role_mapping_periphery.rs:112-127` `review_tier_roles_resolve_to_review`
- `tests/phase_of_role_mapping_periphery.rs:136-151` `implementer_and_any_unrecognized_role_resolve_to_the_fail_visible_build_default`

#### `dup-051d26b56ca0` (semantic, 2 sites)

Proposed home: `one shared `production_main_rs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:35-45` `production_main_rs`
- `tests/store_resolution.rs:27-31` `production_main_rs`

#### `dup-b39b9c66964a` (semantic, 2 sites)

Proposed home: `one shared `start_kurrentdb` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:133-163` `start_kurrentdb`
- `tests/store_resolution.rs:149-180` `start_kurrentdb`

#### `dup-398370d93ff5` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/projections_stay_local.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/projections_stay_local.rs:226-303` `progress_against_the_server_keeps_progress_db_local_and_the_log_on_the_server`
- `tests/store_resolution.rs:196-271` `a_courier_in_a_project_configured_for_the_server_resolves_the_server_store`

#### `dup-a9c4583a7e25` (near, 3 sites)

Proposed home: `proof_lands_on_the_card_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/proof_lands_on_the_card_periphery.rs:355-406` `cross_file_proof_survives_a_real_reextraction_of_the_defining_file`
- `tests/proof_lands_on_the_card_periphery.rs:646-689` `a_deleted_test_reference_retracts_its_stale_proof_through_the_real_pipeline`
- `tests/proof_lands_on_the_card_periphery.rs:720-766` `a_reference_free_tests_dir_files_first_extraction_creates_nothing_and_leaves_no_residue`

#### `dup-d558d72a9443` (near, 4 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:850-863` `bare_remove_dir_all_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:866-882` `bare_git_worktree_remove_with_no_coverage_is_caught`
- `tests/reap_before_removal_audit.rs:1214-1228` `a_finding_names_its_exact_file_and_line_number`
- `tests/reap_before_removal_audit.rs:1343-1370` `a_worktree_remove_args_array_wrapped_across_multiple_lines_by_rustfmt_is_still_caught`

#### `dup-02b6d686ec67` (near, 5 sites)

Proposed home: `reap_before_removal_periphery::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_periphery.rs:43-98` `worktree_remove_reaps_a_process_rooted_in_its_sibling_build_cache_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:118-166` `worktree_remove_reaps_a_process_rooted_in_its_sibling_mutants_root_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:169-229` `discard_reaps_a_process_rooted_in_the_review_worktrees_fence_sibling_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:232-292` `worktree_remove_reaps_a_process_rooted_in_the_cache_dirs_own_store_fence_sibling_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:295-356` `discard_reaps_a_process_rooted_in_the_review_worktree_itself_before_clearing_it`

#### `dup-fc5ffda10df5` (near, 2 sites)

Proposed home: `reminder_dedup_workflow_child_env_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reminder_dedup_workflow_child_env_periphery.rs:83-115` `workflow_stamps_its_own_pid_on_the_spawned_child_with_no_inbound_sentinel`
- `tests/reminder_dedup_workflow_child_env_periphery.rs:124-159` `workflow_still_stamps_a_fresh_own_pid_on_the_child_even_when_its_own_reminder_was_suppressed`

#### `dup-e79f508e24b9` (near, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:260-339` `a_replan_after_a_critique_reject_supersedes_the_initial_episodes_unit`
- `tests/replan_episode_identity.rs:352-430` `a_second_replan_supersedes_both_earlier_episodes_units`

#### `dup-51d311a880da` (near, 3 sites)

Proposed home: `replan_episode_identity::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:623-708` `a_same_id_refine_survives_its_own_episodes_new_sibling_through_the_real_write_path`
- `tests/replan_episode_identity.rs:724-809` `a_same_id_refine_survives_its_own_episodes_new_sibling_walked_first_through_the_real_write_path`
- `tests/replan_episode_identity.rs:824-914` `a_same_id_refine_survives_its_own_episodes_genuinely_new_unmatched_sibling_through_the_real_write_path`

#### `dup-cb850da85ff9` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_compaction.rs:70-96` `rows`
- `tests/reset_derived_compaction_periphery.rs:109-132` `raw_rows`

#### `dup-bd6fce254096` (near, 3 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:86-96` `reset_derived_prunes_when_no_run_has_ever_started`
- `tests/reset_derived_live_writer_guard_periphery.rs:659-669` `reset_force_live_alone_is_refused_as_no_mode`
- `tests/reset_derived_live_writer_guard_periphery.rs:675-690` `the_derived_help_entry_documents_force_live_and_owns_the_risk`

#### `dup-3fde39b380e2` (near, 4 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:102-125` `reset_derived_prunes_when_every_unit_is_terminal_and_every_spawn_is_answered`
- `tests/reset_derived_live_writer_guard_periphery.rs:131-157` `reset_derived_ignores_a_prior_runs_unanswered_spawn_and_non_terminal_unit`
- `tests/reset_derived_live_writer_guard_periphery.rs:573-593` `reset_derived_force_live_compacts_despite_an_in_flight_spawn`
- `tests/reset_derived_live_writer_guard_periphery.rs:621-650` `runs_composed_with_a_refused_derived_still_completes_its_own_prune`

#### `dup-bc312aeb64ee` (near, 2 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:261-292` `reset_derived_refuses_a_non_terminal_unit_between_spawn_rounds_and_prunes_nothing`
- `tests/reset_derived_live_writer_guard_periphery.rs:301-330` `reset_derived_refuses_an_in_flight_spawn_naming_its_id_and_prunes_nothing`

#### `dup-12d8867a4234` (near, 2 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:459-485` `reset_derived_ignores_a_registration_for_a_different_store`
- `tests/reset_derived_live_writer_guard_periphery.rs:504-544` `reset_derived_never_deletes_a_stale_foreign_registry_entrys_file`

#### `dup-6adbc1efd3a6` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/review_round_lenses_only_log_derived_resume_periphery.rs, tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_round_lenses_only_log_derived_resume_periphery.rs:79-103` `spawn`
- `tests/review_round_non_ancestor_residue_names_true_diff_periphery.rs:71-101` `spawn`

#### `dup-e0f2896fa7de` (exact, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3349-3352` `real_files`
- `tests/simplification_audit.rs:4658-4661` `real_map`
- `tests/simplification_audit.rs:6629-6632` `real_workspace_files`

#### `dup-23cb89af01e7` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3407-3424` `dup_cluster_wire`
- `tests/simplification_audit.rs:3426-3439` `dup_cluster_lines`
- `tests/simplification_audit.rs:6582-6599` `dead_code_candidate_wire`
- `tests/simplification_audit.rs:6601-6616` `dead_code_candidate_lines`

#### `dup-352fe3f93801` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6686-6695` `a_simple_free_function_is_found_with_its_line_span`
- `tests/simplification_audit.rs:6886-6892` `production_functions_before_a_cfg_test_mod_are_not_flagged_test`

#### `dup-9484e0875b43` (near, 5 sites)

Proposed home: `simplification_audit::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6800-6806` `a_method_inside_an_impl_block_carries_its_header`
- `tests/simplification_audit.rs:6809-6817` `a_trait_impl_header_keeps_the_trait_for_type_text`
- `tests/simplification_audit.rs:6820-6832` `a_method_inside_an_impl_nested_in_a_cfg_test_mod_is_flagged_test`
- `tests/simplification_audit.rs:6835-6844` `a_cfg_test_attribute_directly_on_an_impl_block_is_flagged_test`
- `tests/simplification_audit.rs:6966-6974` `a_cfg_test_pub_fn_is_still_flagged_test_pub_survives_between_attribute_and_keyword`

#### `dup-810705e338e2` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6857-6863` `a_function_directly_in_a_cfg_test_mod_is_flagged_test`
- `tests/simplification_audit.rs:6866-6875` `a_nested_named_test_submodule_is_still_flagged_test_and_named`

#### `dup-0c08d8eb1d0d` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6986-6990` `a_cfg_test_out_of_line_mod_is_flagged_test`
- `tests/simplification_audit.rs:7064-7069` `an_out_of_line_mod_inherits_test_ness_from_an_enclosing_cfg_test_mod`

#### `dup-23662b623144` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:6993-7004` `a_cfg_test_pub_out_of_line_mod_is_flagged_test_the_real_eventstore_mod_rs_shape`
- `tests/simplification_audit.rs:7007-7025` `a_cfg_all_test_and_feature_compound_out_of_line_mod_is_flagged_test_the_real_blast_radius_eval_shape`
- `tests/simplification_audit.rs:7028-7040` `a_cfg_all_test_pub_mod_is_flagged_test_the_real_eventstore_contract_shape`

#### `dup-a33193324b1e` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7133-7139` `a_method_is_classified_under_its_impl_self_type`
- `tests/simplification_audit.rs:7142-7151` `a_method_on_a_generic_impl_block_strips_the_impls_own_leading_generics`
- `tests/simplification_audit.rs:7217-7222` `a_trait_impl_method_is_classified_under_the_implementing_type_not_the_trait`

#### `dup-52aec5100373` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7281-7313` `section_1_names_every_module_and_every_unassigned_function`
- `tests/simplification_audit.rs:7316-7330` `section_1_reports_none_unassigned_explicitly_when_everything_is_assigned`

#### `dup-821f10171731` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7346-7352` `replace_section_1_only_touches_section_1_leaving_later_sections_intact`
- `tests/simplification_audit.rs:8760-8767` `replace_section_2_only_touches_section_2_leaving_neighbors_intact`
- `tests/simplification_audit.rs:9615-9632` `replace_section_6_only_touches_that_span_leaving_earlier_sections_intact`

#### `dup-f421b9a827dd` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7396-7423` `responsibility_map_json_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:8914-8941` `duplication_catalog_json_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:10865-10892` `dead_code_json_matches_the_tree_or_is_rewritten`

#### `dup-082bc8a1f792` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7462-7486` `a_pin_bump_leaves_the_guarded_responsibility_map_byte_identical`
- `tests/simplification_audit.rs:8991-9018` `a_pin_bump_that_shifts_every_site_in_a_file_leaves_the_guarded_catalog_byte_identical`
- `tests/simplification_audit.rs:10962-10989` `a_pin_bump_leaves_the_guarded_dead_code_json_byte_identical`

#### `dup-0cc7e9bffb3b` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:7512-7578` `two_branches_each_adding_an_unrelated_function_to_a_different_target_file_never_perturb_an_existing_responsibility_map_entry`
- `tests/simplification_audit.rs:10999-11086` `two_branches_each_adding_an_unrelated_function_to_a_different_file_never_perturb_an_existing_dead_code_entry`

#### `dup-8ea9a5680226` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8076-8102` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:8105-8123` `cluster_ids_are_assigned_after_deterministic_sort_and_sites_are_sorted_within_a_cluster`

#### `dup-2a66a04ed48d` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8248-8261` `proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn`
- `tests/simplification_audit.rs:8513-8530` `bespoke_lexer_sweep_finds_the_named_trio_but_not_an_unrelated_fn`

#### `dup-28f7951ffc52` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8268-8281` `the_dash_reap_proc_stat_pair_the_spec_names_lands_in_one_real_cluster`
- `tests/simplification_audit.rs:8385-8400` `the_two_exploration_graph_fixture_builders_the_adversarial_sample_found_land_in_one_real_cluster`

#### `dup-358dae07a664` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8284-8291` `constructs_own_type_literal_matches_self_and_the_named_type_but_not_an_unrelated_call`
- `tests/simplification_audit.rs:8294-8302` `constructs_own_type_literal_matches_shorthand_field_init_too`

#### `dup-0d4baf65b210` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:8403-8431` `same_named_helper_sweep_excludes_required_trait_impl_methods_across_adapters`
- `tests/simplification_audit.rs:8434-8463` `same_named_helper_sweep_excludes_a_trait_default_method_and_its_override`
- `tests/simplification_audit.rs:8466-8490` `same_named_helper_sweep_still_catches_two_inherent_impls_sharing_a_method_name`

#### `dup-4b4a78faea1e` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9270-9301` `report_section_2_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:9463-9508` `report_sections_3_through_5_match_the_tree_or_are_rewritten`

#### `dup-1297ab598508` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9309-9333` `replace_sections_3_to_5_only_touches_that_span_leaving_neighbors_intact`
- `tests/simplification_audit.rs:9336-9353` `replace_sections_3_to_5_falls_back_to_end_of_string_when_no_section_6_heading_exists`

#### `dup-72955e5d1448` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9398-9451` `assert_section_4_structurally_matches`
- `tests/simplification_audit.rs:9710-9763` `assert_section_6_structurally_matches`

#### `dup-3fd4dd52e781` (near, 5 sites)

Proposed home: `simplification_audit::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9830-9840` `resolves_a_same_name_dot_rs_target`
- `tests/simplification_audit.rs:9843-9856` `resolves_a_name_slash_mod_rs_target_when_the_flat_file_does_not_exist`
- `tests/simplification_audit.rs:9859-9875` `resolves_a_path_override_target`
- `tests/simplification_audit.rs:9878-9896` `the_real_eventstore_mod_rs_shape_resolves_contract_rs_as_test`
- `tests/simplification_audit.rs:9899-9920` `transitive_closure_pulls_in_a_second_hop_regardless_of_its_own_local_attribute`

#### `dup-136b8a23c627` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:9978-9989` `resolvers_agree_on_a_same_name_dot_rs_target`
- `tests/simplification_audit.rs:9993-10004` `resolvers_agree_on_a_path_override_target`
- `tests/simplification_audit.rs:10008-10024` `resolvers_agree_on_a_transitive_second_hop`
- `tests/simplification_audit.rs:10028-10036` `resolvers_agree_on_a_non_test_out_of_line_mod`

#### `dup-cc3ade7789a8` (near, 18 sites)

Proposed home: `simplification_audit::support (consolidate these 18 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:10065-10079` `a_fn_referenced_only_by_its_own_test_is_listed`
- `tests/simplification_audit.rs:10082-10096` `a_fn_referenced_from_a_production_caller_does_not_appear`
- `tests/simplification_audit.rs:10145-10157` `a_path_qualified_reference_with_no_call_parens_still_counts`
- `tests/simplification_audit.rs:10160-10170` `a_mention_inside_a_comment_does_not_count_as_a_reference`
- `tests/simplification_audit.rs:10182-10194` `recursion_through_the_fns_own_body_still_counts_as_a_reference`
- `tests/simplification_audit.rs:10197-10214` `a_whole_file_test_via_out_of_line_resolution_is_never_a_candidate`
- `tests/simplification_audit.rs:10234-10253` `a_named_use_import_at_mod_test_top_level_does_not_leak_as_a_production_reference`
- `tests/simplification_audit.rs:10256-10271` `a_serde_default_attribute_string_names_a_real_production_reference`
- `tests/simplification_audit.rs:10543-10556` `a_dot_call_through_an_inherent_method_on_any_receiver_counts_receiver_agnostically`
- `tests/simplification_audit.rs:10577-10592` `an_inherent_associated_function_is_matched_via_path_shape_not_dot_shape`
- `tests/simplification_audit.rs:10667-10681` `a_fn_passed_by_value_as_a_bare_call_argument_counts_as_a_reference`
- `tests/simplification_audit.rs:10698-10711` `a_fn_pointer_used_as_a_struct_literal_field_value_counts_as_a_reference`
- `tests/simplification_audit.rs:10714-10729` `a_ufcs_qualified_value_passed_to_a_combinator_counts_as_a_reference_for_a_method`
- `tests/simplification_audit.rs:10732-10742` `a_fn_named_by_a_let_initializer_counts_as_a_reference`
- `tests/simplification_audit.rs:10745-10756` `a_fn_named_as_an_array_element_counts_as_a_reference`
- `tests/simplification_audit.rs:10759-10770` `a_fn_named_in_a_match_arm_value_counts_as_a_reference`
- `tests/simplification_audit.rs:10773-10783` `a_fn_named_in_a_return_expression_counts_as_a_reference`
- `tests/simplification_audit.rs:10786-10799` `a_fn_named_as_a_generic_argument_to_another_type_counts_as_a_reference`

#### `dup-e13db81454c5` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:10217-10225` `main_is_exempted_as_an_entry_point`
- `tests/simplification_audit.rs:10274-10290` `an_attribute_reference_to_an_ambiguous_shared_name_credits_every_sharer`
- `tests/simplification_audit.rs:10427-10442` `a_method_name_shared_by_two_impls_with_an_unresolvable_receiver_excludes_both`
- `tests/simplification_audit.rs:10559-10574` `a_trait_impl_method_is_exempted_even_with_zero_textual_call_sites`

#### `dup-243981b57f4c` (near, 8 sites)

Proposed home: `simplification_audit::support (consolidate these 8 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:10293-10326` `a_free_fn_bare_name_collision_where_only_one_sharer_has_a_real_caller_flags_the_other`
- `tests/simplification_audit.rs:10329-10351` `a_bare_call_in_the_same_file_as_its_definition_attributes_locally_with_no_import_needed`
- `tests/simplification_audit.rs:10354-10380` `a_bare_unqualified_call_from_a_third_unrelated_file_credits_neither_sharer`
- `tests/simplification_audit.rs:10383-10409` `a_bare_call_resolved_through_a_use_import_attributes_to_the_imported_definition`
- `tests/simplification_audit.rs:10412-10424` `a_method_name_shared_by_two_impls_with_zero_calls_is_flagged_ambiguous`
- `tests/simplification_audit.rs:10595-10606` `an_inherent_associated_fn_name_shared_by_two_types_with_zero_calls_is_ambiguous`
- `tests/simplification_audit.rs:10609-10629` `a_qualified_call_site_attributes_only_to_the_sharer_it_names`
- `tests/simplification_audit.rs:10632-10664` `a_qualified_call_site_on_an_impls_own_generic_self_type_attributes_correctly`

#### `dup-502c3b2e40cb` (near, 2 sites)

Proposed home: `spawn_timing_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spawn_timing_periphery.rs:107-186` `spawn_timing_pairs_real_writer_events_through_a_real_store_by_role`
- `tests/spawn_timing_periphery.rs:283-338` `spawn_timing_excludes_a_real_same_batch_pair_as_suspect_not_a_silent_zero`

#### `dup-7fb3b2dc01eb` (near, 42 sites)

Proposed home: `spec_lint::support (consolidate these 42 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spec_lint.rs:28-76` `validate_spec_reports_every_c3_defect_with_its_criterion_and_field_guide_class`
- `tests/spec_lint.rs:94-137` `validate_spec_attributes_a_prose_level_defect_to_no_criterion`
- `tests/spec_lint.rs:146-183` `validate_spec_reports_two_simultaneous_defects_on_the_same_criterion`
- `tests/spec_lint.rs:192-223` `validate_spec_finds_an_owns_sentence_on_a_wrapped_continuation_line`
- `tests/spec_lint.rs:230-263` `validate_spec_does_not_misread_neither_or_as_either_or`
- `tests/spec_lint.rs:272-306` `validate_spec_ignores_a_smell_phrase_named_in_double_quotes`
- `tests/spec_lint.rs:315-349` `validate_spec_does_not_misread_a_later_or_prefixed_word_as_either_or`
- `tests/spec_lint.rs:358-389` `validate_spec_flags_an_explicit_ownership_denial_as_twin_risk`
- `tests/spec_lint.rs:398-432` `validate_spec_does_not_misread_worth_considering_across_a_hyphenated_compound`
- `tests/spec_lint.rs:441-481` `validate_spec_flags_ownerless_and_not_owned_denials_as_twin_risk`
- `tests/spec_lint.rs:493-532` `validate_spec_does_not_misread_owns_or_owner_inside_an_unrelated_word_as_ownership`
- `tests/spec_lint.rs:544-576` `validate_spec_finds_an_owns_sentence_on_an_unindented_continuation_line`
- `tests/spec_lint.rs:586-624` `validate_spec_does_not_reattach_prose_after_a_blank_line_to_the_prior_criterion`
- `tests/spec_lint.rs:630-657` `validate_reports_a_clean_spec_clean`
- `tests/spec_lint.rs:669-719` `validate_spec_lets_an_affirmative_owns_win_over_an_unrelated_denial_elsewhere`
- `tests/spec_lint.rs:725-756` `validate_spec_recognizes_owner_inside_a_hyphenated_compound`
- `tests/spec_lint.rs:770-803` `validate_spec_lets_a_standalone_owner_win_over_an_unrelated_denial_elsewhere`
- `tests/spec_lint.rs:817-849` `validate_spec_does_not_weld_own_and_er_into_owner_across_a_wrapped_continuation_line`
- `tests/spec_lint.rs:862-887` `validate_does_not_flag_specs_68_own_satisfied_either_or_disposition_clause`
- `tests/spec_lint.rs:895-920` `validate_still_flags_specs_57_genuine_either_or_hedge`
- `tests/spec_lint.rs:930-955` `validate_does_not_pair_a_non_disjunctive_either_with_a_faraway_or_on_specs_68`
- `tests/spec_lint.rs:1150-1179` `validate_still_flags_an_unsatisfied_either_or_as_an_open_hedge`
- `tests/spec_lint.rs:1195-1226` `validate_still_flags_a_genuine_hedge_after_an_earlier_non_disjunctive_either_on_specs_68_shape`
- `tests/spec_lint.rs:1234-1255` `validate_exempts_the_comma_separated_decided_disposition`
- `tests/spec_lint.rs:1262-1283` `validate_still_flags_a_spaced_negation_before_the_decided_idiom`
- `tests/spec_lint.rs:1290-1311` `validate_flags_a_hedge_split_across_hard_wrapped_lines`
- `tests/spec_lint.rs:1323-1346` `validate_does_not_fuse_a_hedge_across_a_heading_boundary`
- `tests/spec_lint.rs:1353-1376` `validate_does_not_fuse_a_hedge_across_a_table_row_boundary`
- `tests/spec_lint.rs:1390-1417` `validate_ignores_a_double_quoted_span_that_crosses_a_hard_wrapped_line`
- `tests/spec_lint.rs:1427-1453` `validate_ignores_a_backtick_span_that_crosses_a_hard_wrapped_line`
- `tests/spec_lint.rs:1468-1493` `validate_still_flags_a_smell_outside_a_balanced_quote_pair`
- `tests/spec_lint.rs:1501-1526` `validate_still_flags_a_smell_outside_a_balanced_backtick_pair`
- `tests/spec_lint.rs:1536-1563` `validate_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `tests/spec_lint.rs:1570-1596` `validate_fails_closed_after_a_stray_unmatched_backtick_earlier_in_the_paragraph`
- `tests/spec_lint.rs:1612-1639` `validate_a_stray_unmatched_quote_does_not_unmask_a_later_real_quoted_disposition_phrase`
- `tests/spec_lint.rs:1659-1685` `validate_ignores_a_backtick_span_whose_closing_mark_is_immediately_after_a_digit`
- `tests/spec_lint.rs:1702-1729` `validate_ignores_a_quoted_span_whose_closing_quote_is_immediately_after_a_digit`
- `tests/spec_lint.rs:1739-1765` `validate_a_digit_adjacent_quote_stays_excluded_as_an_opener`
- `tests/spec_lint.rs:1781-1807` `validate_a_quote_at_the_very_start_of_a_paragraph_is_a_valid_opener`
- `tests/spec_lint.rs:1834-1863` `validate_an_embedded_digit_adjacent_mark_does_not_prematurely_close_a_real_quoted_span`
- `tests/spec_lint.rs:1897-1931` `validate_all_four_digit_adjacency_shapes_together_never_false_positive`
- `tests/spec_lint.rs:1959-1986` `validate_a_digit_glued_to_a_quotes_own_opening_mark_still_masks_the_real_span`

#### `dup-adfce3b42b86` (near, 2 sites)

Proposed home: `step_sheds_the_freshen::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_sheds_the_freshen.rs:128-163` `fingerprint_subtree`
- `tests/step_sheds_the_freshen.rs:129-159` `walk`

#### `dup-8698869c5150` (near, 2 sites)

Proposed home: `store_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_config.rs:51-63` `a_present_store_block_deserializes_backend_and_url`
- `tests/store_config.rs:87-105` `unrelated_workflow_keys_are_ignored_by_the_lightweight_probe`

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

#### `dup-4575c16355c4` (near, 3 sites)

Proposed home: `store_resolution_cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:55-89` `a_server_selected_courier_reaches_the_server_and_never_fabricates_local_sqlite`
- `tests/store_resolution_cli.rs:92-119` `a_courier_with_no_server_configured_resolves_the_local_sqlite_log`
- `tests/store_resolution_cli.rs:122-146` `an_empty_kurrentdb_conn_is_treated_as_unset_not_a_server_with_no_address`

#### `dup-f551cbe69c01` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:72-108` `communities_no_concepts_graph`
- `tests/subject_lens_defined_cells.rs:219-256` `mixed_membership_graph`
- `tests/subject_lens_reprojection_contract.rs:338-389` `file_over_code_graph`

#### `dup-37eae0d8b54b` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells_contract.rs:234-250` `full_in_budget_graph`
- `tests/subject_lens_reprojection_contract.rs:67-110` `community_over_concepts_graph`

#### `dup-b8e6295be9c6` (near, 3 sites)

Proposed home: `subject_lens_reprojection_contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_reprojection_contract.rs:183-232` `reprojection_admits_a_realizing_member_of_any_kind_under_the_concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:481-524` `reprojection_excludes_a_non_code_entity_member_entirely_under_the_code_lens`
- `tests/subject_lens_reprojection_contract.rs:541-591` `reprojection_excludes_a_decision_member_even_when_it_carries_a_live_community_membership`

#### `dup-ac3347889481` (near, 2 sites)

Proposed home: `subject_lens_reprojection_contract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_reprojection_contract.rs:245-286` `reprojection_carries_empty_state_when_no_member_realizes_any_concept_under_the_concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:608-644` `reprojection_carries_empty_state_when_the_sole_realizer_is_purity_excluded`

#### `dup-43938b7413a3` (near, 7 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:297-332` `a_spawn_prompt_carries_the_unified_traversal_code_neighborhood_not_the_old_structural_stitch`
- `tests/unified_traversal_grounding.rs:350-454` `the_implement_prompt_is_trimmed_to_the_intent_layer_with_a_rigger_peers_pointer`
- `tests/unified_traversal_grounding.rs:468-508` `the_producer_prompt_keeps_the_full_grounding_context_not_the_implement_trim`
- `tests/unified_traversal_grounding.rs:656-760` `the_sdet_author_build_seam_spawn_receives_the_trimmed_implement_slice`
- `tests/unified_traversal_grounding.rs:1199-1329` `a_spawn_prompt_carries_the_design_intent_that_governs_the_touched_files_by_traversal`
- `tests/unified_traversal_grounding.rs:1418-1475` `a_governing_decision_never_leaks_into_the_spawn_prompt_design_intent_section`
- `tests/unified_traversal_grounding.rs:1552-1585` `a_spawn_prompt_with_no_governing_design_intent_renders_no_design_intent_header`

#### `dup-a854599a6683` (near, 2 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:990-1033` `the_code_neighborhood_section_is_budget_capped_with_a_visible_elision_note`
- `tests/unified_traversal_grounding.rs:1052-1093` `the_spawn_prompt_code_neighborhood_elision_note_names_the_honest_graph_around_recovery`

#### `dup-10ee2edd03ee` (near, 2 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:1351-1398` `the_design_intent_section_is_budget_capped_and_its_elision_note_names_the_honest_graph_around_recovery`
- `tests/unified_traversal_grounding.rs:1489-1533` `the_spawn_prompt_design_intent_section_renders_the_newest_binding_and_elides_the_oldest`

#### `dup-15e10e5cf0cb` (near, 2 sites)

Proposed home: `validate_advisories::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/validate_advisories.rs:124-149` `validate_warns_of_index_staleness_and_names_reindex`
- `tests/validate_advisories.rs:368-401` `validate_warns_of_graph_index_lag_and_names_reindex`

#### `dup-8605d00c9c2c` (near, 2 sites)

Proposed home: `validate_advisories::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/validate_advisories.rs:404-421` `validate_is_silent_on_graph_index_lag_when_the_graph_matches_the_tree`
- `tests/validate_advisories.rs:424-439` `validate_is_silent_on_graph_index_lag_when_the_graph_has_recorded_nothing`

#### `dup-3e5b7806c96a` (near, 3 sites)

Proposed home: `watchdog_cli_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/watchdog_cli_periphery.rs:55-105` `watch_once_reports_anomalies_through_the_real_compiled_binary_naming_signal_subject_and_response`
- `tests/watchdog_cli_periphery.rs:209-236` `watch_once_reports_a_store_integrity_anomaly_through_the_real_compiled_binary`
- `tests/watchdog_cli_periphery.rs:483-564` `watch_once_reports_the_criterions_own_multi_anomaly_scenario_through_the_real_compiled_binary`

#### `dup-66ad4537aac9` (near, 3 sites)

Proposed home: `watchdog_cli_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/watchdog_cli_periphery.rs:292-359` `watch_without_once_streams_and_re_polls_a_live_mutating_store_until_killed`
- `tests/watchdog_cli_periphery.rs:372-455` `watch_streaming_survives_a_transient_store_read_failure_and_recovers`
- `tests/watchdog_cli_periphery.rs:579-681` `watch_streaming_re_alerts_a_reject_recurrence_churn_count_on_each_increment`

#### `dup-659554f6286f` (near, 2 sites)

Proposed home: `workflow_definition_and_js_constants_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/workflow_definition_and_js_constants_periphery.rs:88-121` `seed_workflow_yml`
- `tests/workflow_definition_and_js_constants_periphery.rs:129-144` `seed_js_files`

### Adversarial sample

Recall check (spec 85 THOROUGHNESS): 30 functions drawn by seeded random index (seed `85072026`, `sample_indices` over all 6842 functions scanned in `src/` and `tests/`, excluding `tests/prioritized_plan_citation_periphery.rs` - criterion 4's own citation-guard periphery test, whose function count grows as its citation-drift-guard mechanism hardens round over round; excluding it keeps that unrelated growth from ever reshuffling this already-verified draw), each read by hand - together with its host file's surrounding context, since a duplicate can live anywhere in the file or a sibling file - to judge whether a duplicate exists that the mechanical pass and the five sweeps above did not already catch.

- `src/blast_radius_eval.rs:377-443` `arm_a_safe_view_is_a_grep_superset_on_the_adversarial_corpus` - no duplicate found by reading
- `src/conductor.rs:29143-29184` `max_retries_for_widens_only_the_resumed_unit_never_a_sibling` - no duplicate found by reading
- `src/conductor.rs:31240-31279` `a_spawned_implementers_title_is_the_unit_criterion` - caught: `dup-975bea08853b`
- `src/conductor.rs:31719-31734` `spawn` - caught: `dup-cd30ba0dffbb`
- `src/conductor.rs:35242-35351` `run_wave_admits_at_most_max_parallel_units_leaving_the_rest_neither_failed_nor_terminal` - no duplicate found by reading
- `src/config.rs:1045-1057` `split_frontmatter` - no duplicate found by reading
- `src/console/map.rs:1888-1907` `frame_always_carries_every_districts_pill_at_every_zoom` - no duplicate found by reading
- `src/dash.rs:9416-9434` `calls_route_walks_both_directions_for_dir_both` - no duplicate found by reading
- `src/dash.rs:10300-10325` `card_of_a_concept_carries_each_top_evidence_members_own_kind` - no duplicate found by reading
- `src/driver/claude_code.rs:1373-1383` `read_fixture_lines` - no duplicate found by reading
- `src/driver/cli.rs:403-430` `persona_is_the_system_prompt_task_is_the_prompt` - caught: `dup-85412eee5dd7`
- `src/grounder/symbols/extract.rs:600-615` `comma_separated_groups` - no duplicate found by reading
- `src/ledger.rs:739-744` `not_done_with_an_escalated_unit` - no duplicate found by reading
- `src/main.rs:8891-8899` `reset_derived` - no duplicate found by reading
- `src/main.rs:13477-13492` `instructions_in_force_line` - no duplicate found by reading
- `src/main.rs:17653-17671` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds` - caught: `dup-d129d6d16af4`
- `src/metrics.rs:1556-1561` `verdict` - no duplicate found by reading
- `src/metrics.rs:1858-1871` `counts_review_rejects_on_both_per_unit_and_fan_out_paths` - caught: `dup-b4de5d907148`
- `src/registry.rs:188-190` `is_stale` - no duplicate found by reading
- `src/registry.rs:465-469` `read_all_over_an_absent_directory_is_an_empty_registry_not_an_error` - no duplicate found by reading
- `src/watch.rs:1090-1104` `assert_dash_probe_is_not_an_anomaly` - no duplicate found by reading
- `tests/cli.rs:9118-9126` `assert_run_level_scratch_reclaimed` - no duplicate found by reading
- `tests/cli.rs:17815-17827` `http_probe` - no duplicate found by reading
- `tests/community_resolution_knob.rs:470-547` `a_firing_rerun_supersedes_only_its_own_resolution_grain` - no duplicate found by reading
- `tests/console_wasm_build_periphery.rs:184-187` `read_recording` - no duplicate found by reading
- `tests/halted_spawn_wip_recovery_periphery.rs:433-496` `a_dirty_tree_gets_no_wip_recovery_commit_while_a_sibling_spawn_of_the_unit_is_still_live` - no duplicate found by reading
- `tests/handbook_grounder_accuracy.rs:160-186` `no_handbook_document_names_a_retired_grounder_as_a_live_choice` - no duplicate found by reading
- `tests/reset_menu.rs:44-57` `seed_one_dead_run_node` - no duplicate found by reading
- `tests/simplification_audit.rs:2476-2483` `workspace_scan_dirs` - no duplicate found by reading
- `tests/simplification_audit.rs:7945-7958` `scan_tree_finds_functions_under_both_src_and_tests_but_not_elsewhere` - no duplicate found by reading

Two real recall gaps surfaced this way and were closed by widening the mechanical sweep with a new generalizable detector each - not a one-off citation - so the fix catches every present and future instance of its class, each pinned by a real-tree regression test: `find_proc_stat_or_status_readers` (decision `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or `/proc/<pid>/status` literal, closing the spec's own named worked example - `src/dash.rs:499-507` `process_state` next to `src/reap.rs:190-197` `pid_starttime`, the same job on the same file with a different field/shape, upheld at spec 62's capstone; `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) groups 2+ non-test functions per `(file, Self type)` that build a `Self { .. }` / `TypeName { .. }` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` reading MISSING from its own `ok`/`failed` cluster even though all three are parallel constructors for one struct. A third worked example, `exploration_graph` (independently defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass with no sweep needed - confirming the mechanical pass itself has real recall, not only the two widened sweeps. Two further real defects, found on review rather than in this draw, were closed the same way: a RECALL gap the architecture lens routed to this criterion by name across two prior review rounds - this file's own bespoke source-text lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter extractor, `src/grounder/symbols/extract.rs::extract` (its own module doc's claim, architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the adversary found by reading every `same-named helper` cluster against `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/`subscribe_stream` across the `EventStore` trait's three backend adapters plus a test double, `blast_radius` across the `Grounder` trait's own default method, its override, and a test double) - closed by excluding members whose extracted Self type differs across the group when at least one comes from an actual `" for "` trait impl (decision `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) excluded this criterion's own citation-guard periphery file from the draw's population (see this subsection's opening paragraph); that exclusion still applies unchanged. A later round's own second commit grew the scanned population from 7513 to 7517 functions, reshuffling the draw; every one of the functions above marked "no duplicate found by reading" was re-read by hand against its host file's surrounding context, exactly as this THOROUGHNESS check requires whenever the draw changes, and all are genuinely not duplicates - this redraw surfaced no new recall gap. Two standing shapes an earlier round's reading pass named, neither drawn this time but both still present and still correctly excluded, are restated here so neither is mistaken for a miss on a future draw: `apply` at `src/conductor.rs:38374-38376` (`grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grep_superset`) is a `Projection` test double's own required trait-impl body, the same port-default/adapter-override/test-double shape `find_same_named_helper_functions`'s trait-impl-precision fix (decision `u85c2-same-named-helper-trait-impl-precision-fix`) already excludes from clustering by design; and `gate_verdict_event` (`src/conductor.rs:37733-37742`) together with the `verdict` closure inside `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` (`src/conductor.rs:39138-39147`) do the identical job - find the recorded `GateVerdict` for a `"<unit>/gate:g#<attempt>"` replay key, panicking with the same message when none exists - differing only in whether the unit segment is the literal `"s"` or a parameter. A `let`-bound closure is not a `fn` item, so no change to this catalog's `fn`-only scanner (module doc, THE SCANNER) short of teaching it to see closures could catalog this pair as a cluster; named here, prominently, rather than silently, so a later refactor - or a scanner that learns to see closures - does not miss it.


## 3. Boundary Violations

Instrument: for each of the five named ports (`eventstore::EventStore`, `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, `grounder::Grounder`), grepped every production (pre-`#[cfg(test)]`) call site of that port's known concrete adapter modules from a NON-adapter, NON-composition-root file, and separately grepped every domain-ish file's top-level `use` statements for a direct infrastructure-crate import. `src/main.rs` is exempt from the "reaches a concrete adapter" check: it is the composition root, and wiring concretions together is its designed job.

FOUND, two violations:

Violation 1 (`AgentDriver`): `src/conductor.rs:7091-7096` (`reclaim_terminal_unit_mutation_scratch`, real production code - well above the `#[cfg(test)] mod tests` boundary this audit's own section 1 identified at `src/conductor.rs:10260`) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` directly by concrete module path. Read via `rigger graph --show AgentDriver`: the port `conductor.rs` actually depends on for driving agents is `trait AgentDriver { fn spawn(&self, agent: &AgentDef, prompt: &str, opts: &SpawnOpts, emit: &dyn Fn(&str, Value) -> Result<(), Error>) -> Result<AgentResult, Error>; }` (`src/conductor.rs:1083-1091`) - one method, `spawn`. Neither called function is about driving an agent or replaying a recorded run (the concern `driver::replay` otherwise owns); both are pure, driver-instance-free scratch-lifecycle utilities that happen to live inside that one concrete adapter's module. The port that should have been used: none exists for this concern yet, which is itself the defect - `conductor.rs` (a use-case/orchestration file) should not need to know which concrete `AgentDriver` implementation happens to define its own mutation-scratch cache-home resolution. Fix direction for a follow-up spec: relocate `cache_home_from` and `reclaim_unit_mutation_scratch` out of `driver::replay` into a neutral, adapter-independent module (a `scratch` or `mutation` support module conductor.rs and every driver adapter can depend on alike), so no use-case file reaches into one specific adapter's internals for a concern that adapter does not conceptually own.

Violation 2 (`Grounder`): `src/ingest.rs:187-211` (`walk_batches`, called from production `conductor::RunCtx::ingest_project_batches` at `src/conductor.rs:7903`, itself called from `src/conductor.rs:7894` well above the `10260` `#[cfg(test)]` boundary) calls `crate::grounder::symbols::events::project_batches_paced` directly by concrete module path at line 197 to reuse the `symbols` grounder's already-persisted index for a one-time whole-project ingest walk, then at line 203 - same function, same missing-port defect, not a separate third violation - calls `crate::grounder::design::events::project_batches` directly by concrete module path for the design-doc half of the same walk. These two calls are two of the three named sites of section 2's own catalogued duplicate cluster (`dup-0f2f14f8c3ce`: `src/grounder/design/events.rs:90-114`, `src/grounder/symbols/events.rs:89-91`, and `src/grounder/workflowdef.rs:245-252` - all three named `project_batches`), so this boundary violation and that duplication finding are two symptoms of one root cause - `ingest.rs` naming each concrete grounder submodule because no port exposes either. The `Grounder` port's own methods (`ground`, `reindex`, `blast_radius`, `index_stamp` - its provenance stamp - all at `src/grounder/mod.rs:133-175`) serve real-time per-query grounding of an agent's prompt; none exposes "hand me every indexed file's projected events for a whole-project batch ingest," so `ingest.rs` - itself a domain ingest authority (its own module doc names it "the ONE walk-and-content-key authority"), not an adapter and not the composition root - has no port to depend on for either call and reaches the concrete `symbols` module (197) and the concrete `design` module (203) directly. Same missing-port defect class as violation 1. Fix direction for a follow-up spec: add an ingest-shaped port method (e.g. a `Grounder::project_batches` or a standalone `SymbolProjector` trait) covering both concrete modules, so `ingest.rs` depends on one abstraction instead of either concrete grounder module for its whole-project walk.

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

### 4.1 Stage 2: the workspace production-reference sweep

A compiler lint never fires on a `pub` item of a crate that is both a library and a binary, so stage 1 cannot see a `pub` function nothing calls. Stage 2 is a reference sweep over the whole workspace instead: the root package's `src/` and every member crate's `crates/<name>/src/`, since a member crate calling into the library is as much a production caller as the library's own binary. Every function defined in that production source is a candidate unless it is test code (a `#[cfg(test)]` span, or a file reached only through a `#[cfg(test)] mod` declaration, transitively), an entry point the language or a foreign caller invokes (a top-level `fn main`, an `extern "C"` export), or a trait-impl method (the language calls `Drop`, formatting, operator and iterator methods with no call site in the text).

THE RULE: a candidate is live when an identifier token equal to its name appears in production code outside its own signature - any token, whatever surrounds it, so a function passed by value, named in an attribute string or used as a path segment counts exactly like a call. `tests/` directories and test spans never count. When several candidates share a name, a reference is attributed to the one it names: a free or associated function by its path qualifier, the referencing file's `use` import, or (for a free function) a bare call in its own file; a method by its receiver's type - the `T` of a `T::name` path, the enclosing impl's type for `self.name` or `Self::name`, or the declared type of a local receiver (a typed parameter, a typed `let`, a struct-literal `let`). A method call whose receiver type cannot be read from the text (a field, a call result, a pattern binding) is credited to every method of that name, so the sweep can miss dead code but never reports live code as dead. Output: `docs/audit/dead-code.json` (line-free, drift-guarded like the duplication catalog) and `docs/audit/dead-code.lines.json` (each entry's line numbers).

### 4.2 The rule: live or deleted

A function is either live or deleted; there is no third state. rigger's library has no consumer outside this workspace - its binary, its integration tests and its member crates all sit inside the sweep's scope - so a function with no production caller has no public surface to protect and no reason to stay: it is deleted together with the tests that exercise only it. Code a future change needs is added by that change, together with its caller. The audit enforces the rule: it fails whenever the ledger below holds any entry, and the failure names each one.

### 4.3 The ledger

The ledger is empty.

### 4.4 Retired-feature remnants and stale doc claims

RETIRED-FEATURE REMNANTS. `turbovec` (spec 57, "Retire turbovec"): grepped the whole tree (`src/`, `tests/`, `docs/`, `specs/`, `Cargo.toml`) for every mention - found only the deliberate migration-error guard code (`src/grounder/mod.rs`'s `is_retired_grounder` / the loud `retired_grounder_error`) plus the tests and docs that keep it retired (`tests/turbovec_retired.rs`, `tests/turbovec_retired_cargo_boundary.rs`, `tests/grounder_name_contract.rs`, and several others naming it as a guarded-against name). Zero implementing code, zero cargo feature, zero dependency - confirmed by reading `Cargo.toml`'s `[features]` section in full (one feature, `symbols`, on by default; no `turbovec` entry anywhere). `kurrentdb` build-time feature flag (spec 47, "KurrentDB is always available"): grepped `Cargo.toml` and every file under `src/` for `feature = "kurrentdb"` / `-F kurrentdb` - zero hits outside the tests that guard against its resurrection (`tests/kurrentdb_always_available.rs`); `kurrentdb` and `tokio` are unconditional `[dependencies]` as spec 47 requires, and `testcontainers` (the adapter's contract-test-only dependency) correctly lives under `[dev-dependencies]`, never the production dependency tree. Both named retirements are fully clean - a real, evidenced negative finding, not an assumption.

STALE DOC CLAIMS. Scanned every `docs/*.md`, `README.md`, and `CONTRIBUTING.md` for any `src/**/*.rs` or `tests/**/*.rs` path-shaped substring and checked each cited path still exists on disk. Two misses surfaced (`src/bar.rs` in `docs/architecture-addendum-pit-of-success.md:252,256`; `src/modifier.rs` in `docs/architecture.md:1022,1027-1028`) - both read in context and confirmed generic illustrative examples in unrelated prose (`crates/foo/src/bar.rs` as a spec-criterion example path, `core-schema/src/modifier.rs` as an event-log worked example), never a real claim about this repository's own layout. Zero genuine dangling file references found.

## 5. Test-Suite Shape

Instrument: subsystem grouping is a hand-derived, ordered filename-keyword rule table (mirrors criterion 1's own per-file classification convention: first-match-wins, narrowest first, an explicit residual named rather than silently dropped). The consolidation notes below cross-reference the ALREADY-COMMITTED `docs/audit/duplication-catalog.json` (criterion 2's own generator output, not re-scanned here) filtered to the 219 clusters whose every site sits under `tests/`; every count in them is read from the catalog at render time.

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
| Grounding (symbols grounder, turbovec retirement, blast radius) | `kurrentdb_always_available.rs` + `turbovec_retired.rs` share 1 duplication cluster confined to just themselves |
| Process lifecycle: no-os-kill & reap discipline |  |
| Event store & config precedence |  |
| Canary (review-panel judge-the-judges evaluation) | `canary_false_positives_periphery.rs` + `canary_unattributed_rejects_periphery.rs` share 2 duplication clusters confined to just themselves; `canary_item_sharding_jobs_cap_periphery.rs` + `canary_progress_hook_periphery.rs` share 0 duplication clusters confined to just themselves |
| Concepts/community lens derivation & fold (non-viz) | `community_detection_cli.rs` + `concepts_derivation_cli.rs` share 0 duplication clusters confined to just themselves |
| Residual (no natural larger home) | `build_budget_slots_periphery.rs`, `gitsemver_derivation.rs` - named rather than forced into an ill-fitting bucket |

### 5.2 Shared fixtures to extract into `tests/common`

`tests/common/mod.rs` and `tests/common/fixtures/` already hold the shared fixtures - the gap is everything still duplicated OUTSIDE them. The catalog's all-helper-function test-only clusters (57 of the 219 test-only clusters) are the evidence; the 4 widest, by distinct files, are the headline case for extraction:

- `write_attention_ordering_workflow` / `write_attention_progression_workflow` / `write_budget_one_dependency_workflow` / `write_budget_one_two_stage_workflow` / `write_failing_gate_escalating_workflow` / `write_liveness_workflow` / `write_manual_review_workflow` / `write_one_stage_workflow` / `write_reviewless_git_escalating_unit_workflow` / `write_reviewless_git_unit_workflow` / `write_solo_unit_workflow` / `write_two_stage_workflow` / `write_unbounded_liveness_workflow` - 15 sites across 6 files (`dup-d29a266a9bb2`, exact).
- `build_harness` - 4 sites across 4 files (`dup-0c9c3cbd4b5a`, semantic).
- `build_harness` - 4 sites across 4 files (`dup-238f6df687da`, near).
- `write_reviewless_git_unit_workflow` - 3 sites across 3 files (`dup-26ae6db7965e`, semantic).

Proposed home for each: `tests/common` (the catalog's own `proposed_home` field says so for each). Consolidating just these collapses roughly 26 duplicate definitions into 4 shared ones - the single largest mechanical simplification this audit identifies anywhere in the test suite.

### 5.3 `tests/cli.rs` split plan

`tests/cli.rs` holds 420 `#[test]` functions. Its existing internal section markers each name the spec and criterion whose tests follow it, not a CLI subcommand or subsystem - ad hoc organization that falls well short of a deliberate, complete per-surface structure. The split proposed below replaces those by-spec markers with a complete, deliberate BY CLI SUBCOMMAND SURFACE organization. A keyword-on-test-name pass (matching each test's dominant CLI verb: `step_`, `run_`, `validate_`, `reset_`, `watch_`/`watchdog_`, `canary_`, `dash_`/`status_`, `store_`/`eventstore_`, `spawn_`/`mutation_scratch_`/`scratch_`, `review_`/`gate_`, `setup_`/`precommit_`/`hook_`, `courier_`/`registry_`, `spec_`, `replay_`, `worktree_`, `emit_`/`peers_`/`decision_`, `stats_`, `heartbeat_`/`liveness_`, `prime_`/`version_`/`init_`) only cleanly covers 238 of the 420 tests (56%) - disclosed honestly rather than overclaimed, because a real fraction of `cli.rs`'s scenarios are DELIBERATELY end-to-end (a single test legitimately drives `step` + `run` + `validate` + `dash` together to prove a cross-cutting property, e.g. `a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status` or `docs_ships_graph_hygiene_guidance_to_consumers`), which a bare keyword match cannot and should not force into one bucket. The proposed split is BY CLI SUBCOMMAND SURFACE - `cli.rs`'s own natural organizing concept, since the whole file drives the `rigger` binary end to end - into per-surface files (`tests/cli_step.rs`, `tests/cli_run.rs`, `tests/cli_validate.rs`, `tests/cli_reset.rs`, `tests/cli_watch.rs`, `tests/cli_canary.rs`, `tests/cli_dash.rs`, `tests/cli_store.rs`, `tests/cli_review.rs`, `tests/cli_setup.rs`, plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios), with each test's home decided by its DOMINANT scenario on a human/AI read, not a mechanical keyword match - the same discipline this audit's own responsibility map applied to unassignable functions (named, never silently forced). Cross-referencing the catalog: `cli.rs` also participates in 10 of the catalog's cross-file test-only duplication clusters, several paired against files that WOULD merge with it under this split (`tests/step_attention_periphery.rs`, paired in 2 clusters; `tests/watchdog_cli_periphery.rs`, paired in 2 clusters) - the split is expected to shrink, not grow, the duplication surface.

### 5.4 Duplicated helpers across test files (beyond 5.2's headline cases)

57 test-only clusters in the committed catalog have every site as an ordinary (non-`#[test]`) helper function - the shared-fixture-extraction candidate class. Beyond the 4 in 5.2, the widest are:

- `fixture_repo` - 3 sites across 3 files (`dup-45864f32842e`, semantic).
- `fixture_graph` - 3 sites across 3 files (`dup-89f6632b3d54`, semantic).
- `temp_project` - 3 sites across 3 files (`dup-a28095f8ff23`, semantic).
- `courier_project` / `courier_project_with_commit` / `temp_project_with_rigger_dir` - 3 sites across 2 files (`dup-8f36b1dc4598`, near).
- `assert_selected_server` / `assert_selected_sqlite` / `assert_server_reached_and_credentials_redacted` - 3 sites across 2 files (`dup-d23f3a1d6a5d`, near).
- `communities_no_concepts_graph` / `file_over_code_graph` / `mixed_membership_graph` - 3 sites across 2 files (`dup-f551cbe69c01`, near).

Every one of these 57 clusters, with its full site list and the catalog's own `proposed_home`, is already machine-readable in the committed `docs/audit/duplication-catalog.json` for a follow-up consolidation spec to consume directly - not re-enumerated exhaustively here to keep this section a report, not a second copy of the catalog.

### 5.5 Table-driven test families

162 test-only clusters have every site as a `#[test]` function - a literal-differs-only-in-input family, spec 85's own named table-driven-test candidate class. The single largest anywhere in the suite: `dup-7fb3b2dc01eb` (near, 42 sites, all in `tests/spec_lint.rs`, e.g. `validate_spec_reports_every_c3_defect_with_its_criterion_and_field_guide_class`, `validate_spec_attributes_a_prose_level_defect_to_no_criterion`, `validate_spec_reports_two_simultaneous_defects_on_the_same_criterion` - near-identical "feed one spec fixture through `validate`, assert one expected defect/advisory line" bodies). Proposed table: `#[test] fn validate_spec_field_guide_defects() { for (fixture, expected) in CASES { ... } }` retiring all 42 named tests into one parametrized loop over a `(&str, &str)` (or richer struct) case table. Other large families (the `tests/reap_before_removal_audit.rs` exemption-coverage family, this generator's own scanner tests and the no-os-kill test helper's pid-refusal tests are already closed: their cases run as `test_cases!` rows over shared case helpers): `dup-a76bc2af4d4b`/`dup-7f174d1fe10b` (11+4 sites, `tests/no_os_kill_audit.rs`, one process-termination-pattern-string per test - a `(pattern, is_caught)` table). As with 5.4, the full 162-family list lives in the committed catalog by cluster id for a follow-up test-consolidation spec to consume directly.

## 6. Prioritized Plan

Twenty follow-up refactoring specs, ordered largest risk-reduction first. This section adds no new findings: every citation below points at a claim already recorded in section 1 (`docs/audit/responsibility-map.json`), section 2 (`docs/audit/duplication-catalog.json`), section 4.3 (`docs/audit/dead-code.json`), or sections 3 and 5's own prose. The three committed JSON files ground every count below (queried directly, never re-scanned). Item 0 (Tier 1) deletes the dead-code ledger (section 4.3); six of the remaining nineteen entries split a god file (tiers 2 and 3, two phases times three files); the other thirteen retire duplication or close a port gap (tiers 1, 4 and 5) - kept as separate entries throughout, per spec 85's own instruction that "the god-file splits and the duplication removals are separate entries so each can be its own run."

### 6.1 How this plan is ordered

Largest risk-reduction first is read as six tiers, ranked by the KIND of risk each entry retires, highest first:

1. Tier 1 - active correctness risk, PLUS item 0: a use case already depends on the wrong concretion, or two independent implementations of one concern can already drift apart silently (section 3's two boundary violations; the one already-drifted `/proc`-reading pair section 2 and section 3 both name) - live gaps, not just size. Item 0 (deleting the dead-code ledger, section 4.3) is placed here too, first of all: not a live-gap risk itself, but the cheapest, zero-behavior-change move available, and it shrinks the files tiers 2 and 3 operate on before either touches them.
2. Tier 2 - god-file test-module extraction: each of the three god files' own inline `#[cfg(test)] mod tests` holds 54-66% of that file's mapped functions (section 1's map), and moving it is a pure relocation with no production-behavior change - the single largest safe line-count reduction in this plan, and the precondition that makes tier 3 tractable.
3. Tier 3 - god-file production splits: section 1's own proposed module tree applied to the (now much smaller) remaining production surface of each god file. Higher execution risk than tier 2 because it touches live orchestration and CLI logic, so it is sequenced after tier 2 shrinks the target first.
4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path literals, sqlite `Connection::open`, error-shaping helpers), each already a single committed cluster with its own proposed home.
5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only duplication. No production-correctness exposure at all (worst case a test regresses, never the product), so it is ordered ahead only of tier 6 despite touching the largest raw line count anywhere in this plan.
6. Tier 6 - remaining catalog sweep: the 289 src-touching clusters section 2 found but tiers 1 and 4 did not individually name. Unlike every other tier, none of these 289 have been read and risk-assessed one at a time the way tiers 1-4's named clusters have - they are consumed straight from the catalog - so this tier carries production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the follow-up spec must triage each cluster's own production-or-test status before merging it, not assume tier 5's blanket test-only treatment applies here too.

Within a tier, entries are ordered largest-first by the site or line count each retires - the same rule the tiers themselves follow, applied one level down.

### 6.2 Tier 1: active correctness risk

#### 0. Delete the dead-code set

- Scope: every entry of section 4.3's ledger (`docs/audit/dead-code.json`) and the tests that exercise only it; a test that also exercises live code is trimmed, not deleted. Under section 4.2's rule no entry is kept, and a function whose only caller was a deleted entry is deleted in the same pass.
- Status: complete - the ledger is empty.
- Risk: low. A deletion is a pure subtraction: `cargo build` and `clippy -D warnings` on both feature lanes catch any missed reference immediately.
- Unblocks: shrinks the files tiers 2-4 operate on before they touch them, so it runs first.

#### 1. Close the `AgentDriver` port gap around mutation-scratch reclaim

- Scope: `conductor.rs`'s production `reclaim_terminal_unit_mutation_scratch` (section 3 violation 1) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` by concrete module path - two pure, driver-instance-free scratch-lifecycle utilities that do not conceptually belong to the `driver::replay` concern they currently live inside. Relocate both into a neutral module every `AgentDriver` adapter and `conductor.rs` can depend on alike (no new trait needed - neither function takes a driver instance, so this is a home fix, not a port-method fix).
- Files: `src/conductor.rs`, `src/driver/replay.rs`, a new home for the two relocated functions.
- Expected line delta: near zero net - a pure move of two functions.
- Risk: low-medium. The reclaim path is covered by spec 83's worktree-lifetime-fenced-by-spawn-liveness contract tests; those tests move with the functions, not get rewritten.
- Unblocks: retires the only `AgentDriver` port violation section 3 found.

#### 2. Close the `Grounder` port gap for whole-project batch ingest (retires dup-0f2f14f8c3ce in the same motion)

- Scope: section 3 violation 2 (`src/ingest.rs::walk_batches`, reaching `grounder::symbols::events::project_batches_paced` and `grounder::design::events::project_batches` by concrete module path) and duplication cluster `dup-0f2f14f8c3ce` (3 modules' own twin `project_batches` functions, in `src/grounder/design/events.rs`, `src/grounder/symbols/events.rs`, `src/grounder/workflowdef.rs`) are one root cause, not two - fix once. 3 CANDIDATES, ONE HOME (spec 85 CONSTRAINTS WALK): `dup-0f2f14f8c3ce`'s own mechanical `proposed_home` suggests relocating into `tests/common`, but every site is production code under `src/grounder/`, not a test helper - the mechanical heuristic has no "add a port method" category to route a production duplicate to, so it mis-fires here. This plan follows section 3's own reasoned disposition instead: add a `Grounder::project_batches` port method (or a standalone `SymbolProjector` trait) covering all 3 concrete modules, and point `ingest.rs` at it.
- Files: `src/ingest.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/events.rs`, `src/grounder/design/events.rs`, `src/grounder/workflowdef.rs`.
- Expected line delta: roughly neutral - one new trait method plus 3 thin impls, minus the 3 duplicate bodies `dup-0f2f14f8c3ce` catalogs.
- Risk: medium. `ingest.rs`'s own module doc calls it "the ONE walk-and-content-key authority" - a load-bearing path; needs the existing whole-project-ingest and reindex-freshening coverage to stay green, not just the duplicate sites' own tests.
- Unblocks: retires the one `Grounder` port violation section 3 found and `dup-0f2f14f8c3ce` together, rather than as two separately-tracked fixes.

#### 3. Retire the duplicate `/proc`-reading authority (`dup-6f592e6f36fa` + `dup-1a85bca8b768`)

- Scope: `src/dash.rs::process_state` independently re-derives `/proc/<pid>/stat` fields that `src/reap.rs` (`pid_starttime`/`read_ppid`) already parses - the exact "second mutation authority" example spec 85's own Goal names and spec 62's capstone previously caught (`dup-1a85bca8b768`, 15 sites across `src/dash.rs`, `src/reap.rs`, `tests/claude_code_stream_periphery.rs`, `tests/cli.rs`, `tests/common/fixtures/host.rs`, `tests/mutation_runner_pdeathsig_periphery.rs`, `tests/simplification_audit.rs`), plus 56 raw `/proc`-path string literals scattered across 9 files with no shared composer (`dup-6f592e6f36fa`). Both clusters' own `proposed_home` agree: `src/reap.rs` becomes the one `/proc`-reading module; `dash.rs` calls it instead of re-parsing. `process_state` is reachable from `dash`'s own always-on production server, so it is the actual active-correctness risk this tier-1 placement is about; the cluster's test-only readers (such as the shared `tests/common/fixtures/host.rs::pgid_of` fixture) earn no tier-1 placement on their own and ride in this same item only because they share its root cause.
- Files: `src/dash.rs`, `src/reap.rs`, plus the test-only readers `dup-1a85bca8b768` names.
- Expected line delta: negative - retires `process_state`'s own parsing body in favor of calling `reap.rs`'s existing parser.
- Risk: low. Section 3's own disposition already establishes `process_state` as a duplicate READ-only reimplementation, never a bypassed mutation path - nothing this touches can signal or end a process, so it carries none of the no-os-kill gate's own risk surface; retiring the test-only readers is ordinary test cleanup.
- Unblocks: retires the codebase's only currently-known live instance of the "duplicate implementation reconciled after the fact" pattern the operator's strict-DRY rule targets - the concrete precedent spec 85's own Goal cites.

### 6.3 Tier 2: god-file test-module extraction

Each god file's inline test module is the set of its `is_test: true` entries in the committed `docs/audit/responsibility-map.json`. Each entry below moves an already-passing test module with no intended production-behavior change - a `cargo test` pass before and after is the whole verification. The line figures below sum those test functions' own spans, so they exclude the module-level doc comments, `use` statements and blank lines around them.

#### 4. Extract `src/conductor.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 498 of its 745 mapped functions (66%), roughly 29361 lines of test-function spans. Partition into a `src/conductor/tests/` directory, one file per concern, reusing the same names section 1 already assigned the file's own production buckets (`run_ctx`, `support`, `gate`, `schedule`, `run`, ...) so the split needs no new naming scheme.
- Files: `src/conductor.rs` -> `src/conductor.rs` (production only) + `src/conductor/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 29361 lines relocated out of `src/conductor.rs`.
- Risk: low - mechanical move of passing tests, zero intended behavior change.
- Unblocks: shrinks `conductor.rs` to its production code before tier 3 touches a single production line, cutting the odds that an unrelated future unit's blast radius collides with this file.

#### 5. Extract `src/main.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 383 of its 702 mapped functions (54%), roughly 10892 lines of test-function spans. Same partition approach as item 4, reusing section 1's own production bucket names (`commands`, `store`, `support`, `provenance`, `render`, ...).
- Files: `src/main.rs` -> `src/main.rs` (production only) + `src/main/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 10892 lines relocated out of `src/main.rs`.
- Risk: low, same rationale as item 4.
- Unblocks: shrinks `main.rs` to its production code before tier 3's own main.rs split.

#### 6. Extract `src/dash.rs`'s inline test module

- Scope: the file's `#[cfg(test)] mod tests` holds 149 of its 241 mapped functions (61%), roughly 5629 lines of test-function spans. Lower effort than items 4-5: section 1's own classifier already found 5 pre-existing sub-boundaries inside this one test module (`dash::tests::calls_route_c4`, `dash::tests::metadata_card_c2`, `dash::tests::rationale_overlay_c3`, `dash::tests::subject_view_c5`, `dash::tests::supervised_lifecycle`), so the partition points already exist and need only become their own files.
- Files: `src/dash.rs` -> `src/dash.rs` (production only) + `src/dash/tests/*.rs`.
- Expected line delta: 0 net (repo-wide) - roughly 5629 lines relocated out of `src/dash.rs`.
- Risk: low - the lowest-effort of the three, for the reason above.
- Unblocks: shrinks `dash.rs` to its production code before tier 3's own dash.rs split.

### 6.4 Tier 3: god-file production splits

Each entry below applies section 1's own proposed module tree to a god file's production surface, sequenced after the matching tier-2 entry removes that file's test bulk first. Every module name and function/line count below is summed directly from the committed `docs/audit/responsibility-map.json` (function-body spans only); a file's remaining non-function production lines - struct/enum/type definitions, `use` statements, module docs - are outside section 1's own function-only scan and move with whichever module they sit beside, without needing their own assignment.

#### 7. Split `src/conductor.rs`'s production code into `src/conductor/*.rs`

- Scope: 213 mapped functions across 15 proposed modules (roughly 7958 lines of function bodies) plus 34 unassigned functions (1225 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `conductor::run_ctx` (125/6619), `conductor::support` (22/525), `conductor::gate` (19/183), `conductor::schedule` (8/130), `conductor::run` (6/122).
- Files: `src/conductor.rs` -> `src/conductor/mod.rs` + `src/conductor/{run_ctx,support,gate,schedule,run,review,prior_failure,budget,spawn,emit,agent_failure,review_outcome,ground,gate_ratchet,integration_approval}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 9183 lines of function bodies.
- The largest bucket, `conductor::run_ctx`, may warrant its own second pass if it does not decompose cleanly into one file.
- Risk: medium-high - conductor.rs is the composition root's own most complex use-case file; every intermediate commit needs the full `cargo test`, no-os-kill and reap audits green, not just the final one.
- Unblocks: the largest reduction in production-code blast-radius collision risk this audit identifies; makes future duplication-spotting against conductor.rs's own logic tractable by a human reviewer, not only by the mechanical scanner.

#### 8. Split `src/main.rs`'s production code into `src/main/*.rs`

- Scope: 206 mapped functions across 15 proposed modules (roughly 6605 lines of function bodies) plus 113 unassigned functions (3040 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `main::commands` (41/3103), `main::store` (33/926), `main::support` (36/834), `main::provenance` (22/470), `main::render` (10/352).
- Files: `src/main.rs` -> `src/main.rs` (composition root, thinned) + `src/cli/{commands,store,support,provenance,render,dash_glue,setup,liveness,replay_runner,store_location,run_registration,docs_overlay,scaffold_report,residue_report,store_selection}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 9645 lines of function bodies.
- Risk: medium - `main.rs` is the composition root itself; the split must preserve which concretions get wired where, not merely move text.
- Unblocks: shrinks `main.rs` to a genuine composition root plus a `cli/` module tree, matching the ports-and-adapters shape this project already mandates everywhere else.

#### 9. Split `src/dash.rs`'s production code into `src/dash/*.rs`

- Scope: 50 mapped functions across 7 proposed modules (roughly 1339 lines of function bodies) plus 42 unassigned functions (800 lines, each individually named in the committed map for manual placement, per section 1's own "unassignable functions are named as such, never omitted" rule). Headline buckets (functions/lines): `dash::server` (8/611), `dash::render` (18/402), `dash::reproject` (5/176), `dash::registry` (6/65), `dash::response` (5/49).
- Files: `src/dash.rs` -> `src/dash/mod.rs` + `src/dash/{server,render,reproject,registry,response,reaped_child,dash_marker}.rs`.
- Expected line delta: 0 net - pure relocation of roughly 2139 lines of function bodies.
- Risk: low-medium - the always-on dash's own contract (loopback-only, zero-new-dependency) is unaffected by a pure module split.
- Unblocks: completes the god-file split trio; the third program-sized file becomes an ordinary module tree.

### 6.5 Tier 4: named production duplication sweeps

Each entry is one of section 2's five named mandatory sweeps - collected mechanically regardless of the Jaccard pass, per spec 85's own Design.

#### 10. Consolidate the 673 `.rigger`-path string-literal sites (`dup-8da2bd340832`) - the single largest cluster in the entire catalog by site count

- Scope: one `.rigger`-relative path-composition helper (the cluster's own `proposed_home`) every one of the 673 sites routes through instead of building its own literal.
- Files: spans dozens of files including `src/conductor.rs`, `src/config_store.rs`, `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list is in the committed `docs/audit/duplication-catalog.json` under `dup-8da2bd340832` for the follow-up spec to consume directly, not re-enumerated here.
- Expected line delta: negative - 673 literal compositions collapse toward one helper's call sites; the helper itself is small.
- Risk: medium - the largest surface-area sweep in this plan by site count, even though each individual site is trivial; needs a mechanical rewrite pass plus a full-suite green run, not hand-editing 673 sites.
- Unblocks: the biggest single site-count reduction available anywhere in the duplication catalog.

#### 11. Consolidate the 265 `Command::new` call sites (`dup-124a486a7883`) behind one injected process-spawn port

- Scope: one process-spawn seam every `Command::new` site routes through (the cluster's own `proposed_home`).
- Files: spans `src/budget.rs`, `src/conductor.rs`, `src/dash.rs`, `src/driver/cli.rs`, `src/gate.rs`, `src/main.rs`, `src/worktree.rs` plus many `tests/` files - full site list in `docs/audit/duplication-catalog.json` under `dup-124a486a7883`.
- Expected line delta: negative, though smaller per-site than `dup-8da2bd340832` since each `Command::new` call already carries real configuration (args, env, cwd) that must move with it, not just a literal.
- Risk: medium-high - several of these 265 sites sit inside `src/budget.rs`'s and `src/conductor.rs`'s already-hardened process-lifecycle code (spec 78's no-os-kill discipline); the follow-up spec must preserve every existing handle-bound-kill invariant at each site it touches, and the no-os-kill gate is the acceptance bar, not merely `cargo test`.
- Unblocks: one seam instead of 265 independent constructions - the next process-spawning concern added anywhere in the crate reuses it instead of adding one more.

#### 12. Consolidate the 39 sqlite `Connection::open` call sites (`dup-e4cc0ac793e5`)

- Scope: one sqlite-connection-opening adapter function (the cluster's own `proposed_home`) spanning `src/contextgraph/sqlite.rs`, `src/eventstore/sqlite.rs` and `src/main.rs`, plus several `tests/` files.
- Files: full site list in `docs/audit/duplication-catalog.json` under `dup-e4cc0ac793e5`.
- Expected line delta: negative - 39 open calls collapse toward one function.
- Risk: medium - touches the event store and context graph's own connection-lifecycle code; needs the store-identity and store-resolution contract tests green throughout.
- Unblocks: one place to change pragma/timeout/journal-mode settings instead of 39.

#### 13. Consolidate the 12 error-shaping helper sites (`dup-83a6d4922163`) - caution, confirm before merging

- Scope: the cluster spans `src/conductor.rs` (`no_result_error`, `guard_review_round_tree_on_tier_err`, `integrate_plan_commits_wraps_any_hard_error_with_the_plan_landing_marker`), `src/grounder/mod.rs` (`retired_grounder_error`), `src/worktree.rs` (`land_reports_a_generic_error_for_a_refusal_that_is_neither_tip_moved_nor_blocked`, `revert_on_base_aborts_and_errors_on_a_conflicting_revert`) and 6 unrelated test files - a wide spread for one claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own CONSTRAINTS WALK: "the threshold is a floor for the mechanical pass; the reading pass owns semantic duplicates") rather than one real shared concern - the follow-up spec's first job is confirming by reading whether these 12 sites share actual logic before proposing one helper, not assuming the cluster label proves it.
- Files: the `src/` files above, plus the 6 test files named in `docs/audit/duplication-catalog.json` under `dup-83a6d4922163`.
- Expected line delta: unknown pending the confirmation read above - potentially zero if the cluster does not survive a human read.
- Risk: low (the smallest-site-count sweep), but with the stated precondition.
- Unblocks: either a genuine fifth consolidation, or a documented "not a real duplicate" disposition that keeps the catalog honest for whoever reads it next.

### 6.6 Tier 5: test-suite consolidation

Every entry cites section 5's own already-catalogued test-only duplication; none of it carries production-correctness risk.

#### 14. Extract the headline shared test fixtures into `tests/common` (section 5.2)

- Scope: `dup-d29a266a9bb2` (6 files), `dup-0c9c3cbd4b5a` (4 files), `dup-238f6df687da` (4 files), `dup-26ae6db7965e` (3 files) - roughly 26 duplicate definitions collapsing into 4 shared ones, the single largest mechanical simplification section 5 identifies anywhere in the test suite.
- Files: per-cluster, from the committed catalog, plus `tests/common/`.
- Expected line delta: negative - each fixture's small body survives once instead of once per file.
- Risk: low - test-only, and `tests/common/` already holds the same shape of shared fixture.
- Unblocks: item 17 below (the remaining test-helper clusters) reuses the same `tests/common` home this item establishes.

#### 15. Split `tests/cli.rs` by CLI subcommand surface (section 5.3's plan)

- Scope: 420 tests, split into `tests/cli_{step,run,validate,reset,watch,canary,dash,store,review,setup}.rs` plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios section 5.3 names, using each test's dominant scenario (a human/AI read, not the 56%-coverage keyword match section 5.3 already disclosed as insufficient alone).
- Files: `tests/cli.rs` and the eleven new files above.
- Expected line delta: 0 net - pure relocation into eleven files.
- Risk: low-medium - a mechanical per-test move with `cargo test`'s full pass count as the verification.
- Unblocks: splits a file in 10 cross-file duplication clusters (section 5.3) and lets item 17's remaining-clusters sweep target smaller, subcommand-scoped files.

#### 16. Convert the two remaining largest table-driven test families into parametrized tables (section 5.5)

- Scope, largest first: `dup-7fb3b2dc01eb` (42 sites, `tests/spec_lint.rs`), `dup-a76bc2af4d4b`/`dup-7f174d1fe10b` (11+4 sites, `tests/no_os_kill_audit.rs`) - 57 sites across 3 clusters (the reap audit's exemption-coverage family, this generator's own scanner tests and the no-os-kill test helper's pid-refusal tests are already closed).
- Files: the two files named above.
- Expected line delta: negative - each family's near-identical test bodies collapse into one parametrized loop over a table.
- Risk: low - test-only, and each family already shares one body shape (section 5.5's own finding).
- Unblocks: the largest reduction in raw `#[test]` count available in the suite (roughly 57 named tests retiring toward 2).

#### 17. Sweep the remaining 53 test-only helper-duplication clusters (section 5.4, beyond item 14's headline fixtures)

- Scope: the 57 test-only, all-helper-function clusters section 5.4 names, minus the ones item 14 already covers - consumed directly from `docs/audit/duplication-catalog.json`, not re-enumerated here (section 5.4's own stated approach).
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative across 53 clusters.
- Risk: low - test-only.
- Unblocks: closes out the helper-duplication half of the test suite's own strict-DRY exposure.

#### 18. Sweep the remaining 159 table-driven test families (section 5.5, beyond item 16's headline families)

- Scope: the 162 test-only, all-`#[test]` clusters section 5.5 names, minus the 3 cluster ids item 16 already covers - consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative.
- Risk: low - test-only.
- Unblocks: closes out the table-driven-test half of the test suite's own strict-DRY exposure; combined with items 14 and 16-17, retires all 219 test-only clusters section 2 found.

### 6.7 Tier 6: remaining catalog sweep

Unlike tier 5, this entry's own clusters are NOT known to be test-only - each one needs its own read before merging (see `### 6.1`'s tier 6 rationale above).

#### 19. Sweep the remaining 289 src-touching duplication clusters (section 2, beyond tiers 1 and 4's 7 named clusters)

- Scope: of the catalog's 515 clusters, 219 are test-only (items 14 and 16-18 above) and 7 are the named tier-1/tier-4 items (`dup-124a486a7883`, `dup-8da2bd340832`, `dup-e4cc0ac793e5`, `dup-6f592e6f36fa`, `dup-1a85bca8b768`, `dup-0f2f14f8c3ce`, `dup-83a6d4922163`); the remaining 289 clusters touching `src/` - mostly small 2-5-site exact/near matches like the two worked examples section 2 itself opens with (`dup-7dfa0dd1a6a8`, `dup-32656e0a4e12`) - are swept here, largest exact-duplicate clusters first, consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative; the largest single contributor is whichever exact cluster has the most sites (read from the catalog at spec-writing time, not fixed here).
- Risk: low-medium - unlike tier 5, some of these clusters are production code, so each merge needs its own test-coverage check, not a blanket "test-only" pass.
- Unblocks: the last of the catalog's 515 clusters; after items 1-3 and 10-19 all land, a future spec can state and check that the duplication catalog's own drift guard finds zero live clusters left unaddressed.

### 6.8 Dead and vestigial code beyond item 0: no further follow-up

Section 4.2's rule leaves no dead-code category for a later plan item: a function is live or it is deleted by item 0. Both named retirements (`turbovec`, `kurrentdb`) are still fully clean, and the two stale-looking doc paths found remain confirmed generic illustrative examples, not real dangling references.
