# Simplification Audit - 2026-09

The audit report spec 85 derives the follow-up refactoring specs from. Six sections, each claim citing `file:line`; see `specs/85-simplification-audit.md` for scope.

## 1. Responsibility Map

Every function in `src/conductor.rs`, `src/main.rs` and `src/dash.rs` (1504 functions total), assigned to a proposed module by `tests/simplification_audit.rs`'s deterministic scanner + rule-table classifier (never by hand). Instrument: the brace-matching scanner over the three named files.

### Proposed module tree

- `conductor::budget` (5 functions)
  - `src/conductor.rs:325-327` `compensation_queued_key` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:842-871` `pending_compensations_from_log` - name contains "compensat" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:1150-1154` `budget_refused` - name contains "budget" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:1159-1161` `is_budget_refused` - name contains "budget" (budget accounting); grouped under `conductor::budget`.
  - `src/conductor.rs:9824-9834` `mutation_scratch_settled` - name contains "mutation" (budget accounting); grouped under `conductor::budget`.
- `conductor::error` (3 functions)
  - `src/conductor.rs:655-657` `from` - method inside `impl From<crate::eventstore::Error> for Error`; grouped with its other `Error` methods.
  - `src/conductor.rs:660-662` `from` - method inside `impl From<crate::worktree::Error> for Error`; grouped with its other `Error` methods.
  - `src/conductor.rs:665-667` `from` - method inside `impl From<serde_json::Error> for Error`; grouped with its other `Error` methods.
- `conductor::gate` (19 functions)
  - `src/conductor.rs:291-293` `gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:303-305` `gate_skip_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:316-318` `postmerge_gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:335-342` `gate_intersects_radius` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:505-507` `unit_of_gate_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:515-519` `gate_key_attempt` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:586-616` `recorded_gate_outcome` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:621-623` `deferred_gate_verdict_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:632-634` `deferred_gate_failed_key` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:1265-1275` `verdict_channel_mismatch` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:1280-1282` `is_verdict_channel_mismatch` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:8624-8631` `with_gate_hold` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:8754-8761` `gate_failure_cause` - name contains "gate" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:8773-8775` `verdict_approves` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:8791-8800` `last_verdict` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:8807-8809` `has_verdict_line` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:8816-8818` `emitted_verdict_approves` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:8828-8840` `verdict_compensates` - name contains "verdict" (gate execution); grouped under `conductor::gate`.
  - `src/conductor.rs:10109-10117` `critique_gate_name` - name contains "gate" (gate execution); grouped under `conductor::gate`.
- `conductor::gate_ratchet` (1 function)
  - `src/conductor.rs:755-761` `for_persistent_failure` - method inside `impl GateRatchet`; grouped with its other `GateRatchet` methods.
- `conductor::ground` (1 function)
  - `src/conductor.rs:9311-9320` `graph_around_recovery` - name contains "graph" (grounding integration); grouped under `conductor::ground`.
- `conductor::integration_approval` (1 function)
  - `src/conductor.rs:895-897` `approved` - method inside `impl IntegrationApproval`; grouped with its other `IntegrationApproval` methods.
- `conductor::prior_failure` (3 functions)
  - `src/conductor.rs:916-920` `is_empty` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `src/conductor.rs:924-936` `summary` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
  - `src/conductor.rs:941-971` `block` - method inside `impl PriorFailure`; grouped with its other `PriorFailure` methods.
- `conductor::review` (9 functions)
  - `src/conductor.rs:444-491` `route_review_tier` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:1219-1232` `degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:1237-1239` `is_degenerate_reviewer` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:8715-8724` `review_evidence` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:8919-8926` `review_protocol` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:9728-9733` `review_worktree_dir` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:9740-9742` `review_branch` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:10215-10217` `review_roster` - name contains "review" (review orchestration); grouped under `conductor::review`.
  - `src/conductor.rs:10223-10229` `adjudicator_roster` - name contains "adjudicat" (review orchestration); grouped under `conductor::review`.
- `conductor::review_outcome` (2 functions)
  - `src/conductor.rs:787-794` `approved` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
  - `src/conductor.rs:795-802` `rejected` - method inside `impl ReviewOutcome`; grouped with its other `ReviewOutcome` methods.
- `conductor::run` (6 functions)
  - `src/conductor.rs:9700-9702` `unit_branch` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:9712-9718` `unit_worktree_dir` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:10017-10030` `stale_units_from_log` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:10052-10090` `stale_downstream_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:10124-10142` `unit_slug` - name contains "unit" (unit run loop); grouped under `conductor::run`.
  - `src/conductor.rs:10152-10191` `baseline_units` - name contains "unit" (unit run loop); grouped under `conductor::run`.
- `conductor::run_ctx` (100 functions)
  - `src/conductor.rs:2428-2430` `emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2439-2454` `append_and_fold` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2466-2488` `append_and_fold_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2493-2499` `emit_with_actor` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2509-2517` `emit_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2530-2532` `emit_keyed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2543-2568` `emit_keyed_meta` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2603-2642` `emit_keyed_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2651-2657` `agent_model` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2672-2674` `recorded_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2684-2686` `cached_green_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2693-2695` `is_stale` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2720-2731` `effective_attempts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2742-2750` `gate_verdict_high_water` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2758-2772` `mark_stale_downstream` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2788-2866` `drain_compensations` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2875-2908` `commits_to_compensate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2918-2949` `emit_gate_skip` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:2966-3025` `emit_gate_verdict` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3033-3040` `max_retries` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3066-3072` `budget_tripped` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3079-3081` `spawn_is_recorded` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3097-3120` `reserve_spawn` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3123-3125` `budget_broke` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3131-3133` `parked` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3139-3141` `manual_review_pending` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3146-3148` `budget_halted` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3157-3187` `trip_budget_breaker` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3198-3208` `halt_reason` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3213-3223` `check_coverage_or_flag` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3225-3329` `run_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3338-3372` `partition_wave` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3377-3387` `partition_requested` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3392-3416` `run_batch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3418-3455` `start_and_run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3457-3565` `run_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3570-3587` `stage_paused_for_review` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3593-3599` `effective_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3611-3618` `select_review_panel` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3630-3652` `log_review_tier` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3666-3684` `assert_isolated_cwd` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3699-3755` `reviewer_spawn_opts` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3825-3943` `review_unit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:3980-4041` `spawn_sdet_author` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4047-4679` `run_single_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4685-4692` `effective_speculation_width` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4701-4708` `speculates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4717-4733` `speculation_lane_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:4754-5114` `run_speculation` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5127-5206` `emit_speculation_winner_status` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5225-5262` `record_speculation_reject` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5272-5305` `cancel_speculation_candidates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5307-5360` `run_fan_out_stage` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5370-5536` `run_fan_out_review_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5567-5619` `run_review_agents_concurrently` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5631-5662` `run_lens` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5701-5841` `run_reviewer` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5877-5894` `gating_spawn_emitted_approve` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5916-5926` `review_spawn_errored` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5950-5983` `reviewer_result_is_degenerate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:5998-6028` `run_adversary` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6046-6083` `run_adjudicator` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6105-6126` `dag_unit_blast_radii` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6135-6214` `build_dag_critique_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6223-6314` `re_plan` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6325-6358` `run_plan_critique_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6368-6534` `plan_critique_loop` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6562-6573` `build_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6598-6604` `spawn_env` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6615-6620` `build_budget` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6645-6653` `shared_build_cache_paths` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6664-6867` `run_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:6883-6971` `run_gate_with_taxonomy` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7028-7035` `gc_integrated_branches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7042-7145` `gc_integrated_branches_logged` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7181-7188` `reclaim_terminal_unit_mutation_scratch` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7218-7399` `run_deferred_gates` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7410-7477` `record_gate` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7479-7615` `integrate_and_emit` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7624-7626` `build_system_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7636-7648` `ground_query` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7654-7664` `implementer_agent` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7668-7692` `plan_protocol` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7720-7754` `grounded_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7770-7783` `grounded_seed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7807-7836` `record_blast_radius` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7838-7844` `build_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7855-7857` `build_review_prompt` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7865-7899` `build_prompt_with_failure` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7901-7965` `graph_context` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7978-7986` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:7994-8053` `ingest_project_batches` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8058-8058` `ingest_project_into_graph` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8060-8075` `emit_lesson` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8080-8086` `agent_isolated` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8104-8130` `resume_phase` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8132-8164` `stage_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8179-8201` `review_only_worktree` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8203-8569` `harvest_proposed` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
  - `src/conductor.rs:8591-8613` `resolve_served_criterion` - method inside `impl RunCtx<'_>`; grouped with its other `RunCtx` methods.
- `conductor::schedule` (6 functions)
  - `src/conductor.rs:8655-8657` `normalize_criterion_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:8676-8681` `criterion_stable_id` - name contains "criterion" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:9846-9868` `partition_by_blast_radius` - name contains "partition" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:9886-9904` `partition_with_serialize` - name contains "partition" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:9919-9939` `blast_radius_conflicts` - name contains "blast" (unit scheduling); grouped under `conductor::schedule`.
  - `src/conductor.rs:10288-10302` `ready_stages` - name contains "stage" (unit scheduling); grouped under `conductor::schedule`.
- `conductor::spawn` (4 functions)
  - `src/conductor.rs:1112-1116` `parked_spawn` - name contains "spawn" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1121-1123` `is_parked` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:1175-1177` `is_parked_or_budget_refused` - name contains "parked" (spawn lifecycle); grouped under `conductor::spawn`.
  - `src/conductor.rs:10276-10286` `wave_ready` - name contains "wave" (spawn lifecycle); grouped under `conductor::spawn`.
- `conductor::support` (15 functions)
  - `src/conductor.rs:386-388` `path_is_high_risk` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:2113-2115` `has_producer` - name contains "has_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:8640-8642` `normalize_ws` - name contains "normalize" (normalization helper); grouped under `conductor::support`.
  - `src/conductor.rs:8892-8894` `build_system_prompt` - name contains "build" (generic construction helper); grouped under `conductor::support`.
  - `src/conductor.rs:9127-9271` `write_capped_section` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:9322-9378` `write_code_neighborhood` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:9491-9574` `write_design_intent` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:9580-9595` `write_capped_decisions` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:9600-9617` `write_capped_lessons` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:9625-9650` `write_capped_findings` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:9659-9667` `write_peers_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:9682-9692` `write_lookup_pointer` - name contains "write" (generic write helper); grouped under `conductor::support`.
  - `src/conductor.rs:9951-9953` `is_fan_out` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:9974-9976` `is_producer` - name contains "is_" (predicate helper); grouped under `conductor::support`.
  - `src/conductor.rs:10235-10237` `has_llm_verifier` - name contains "has_" (predicate helper); grouped under `conductor::support`.
- `conductor::tests` (418 functions)
  - `src/conductor.rs:2378-2424` `for_test` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10361-10380` `unit_worktree_dir_derives_deterministically_from_scratch_root_and_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10383-10470` `recorded_gate_outcome_reads_the_latest_gate_run_verdict_per_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10386-10395` `verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10473-10500` `review_worktree_dir_and_branch_derive_from_stage_and_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10592-10617` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10620-10627` `prompts_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10630-10637` `dirs_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10641-10647` `system_prompt_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10651-10653` `title_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10657-10659` `reviews_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10663-10665` `spawn_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10669-10676` `spawn_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10680-10686` `spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10692-10699` `dir_existed_when_spawned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10702-10807` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10810-10815` `agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10820-10826` `agent_with_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10828-10834` `gate_def` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10839-10845` `gate_def_inputs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10848-10874` `integrates_a_passing_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10877-10901` `coverage_gate_refuses_an_uncovered_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10904-10936` `planner_extends_the_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:10939-11005` `planner_proposed_unit_with_a_coverage_criterion_runs_and_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11008-11096` `conductor_creates_one_baseline_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11099-11221` `producer_prompt_carries_the_criteria_and_plan_protocol_grounded_on_the_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11226-11228` `baseline_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11233-11260` `supersede_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11263-11312` `a_prior_runs_proposal_never_resurrects_in_a_new_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11315-11397` `planner_unit_supersedes_the_matching_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11400-11436` `a_criterion_with_no_planner_unit_keeps_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11439-11512` `planner_refinement_split_is_still_harvested` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11515-11648` `a_same_id_re_emit_updates_the_proposed_unit_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11655-11685` `seed_refine_dag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11689-11709` `append_proposals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11712-11785` `a_coverage_retarget_re_emit_preserves_one_unit_per_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11788-11848` `a_re_emit_naming_a_reserved_stage_never_mutates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11851-11916` `re_folding_a_same_id_re_emit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:11919-12026` `a_real_split_two_distinct_ids_both_survive_the_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12029-12125` `a_later_episodes_proposal_supersedes_an_earlier_episodes_planner_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12128-12185` `a_same_episode_re_seen_on_a_later_fold_still_never_self_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12188-12267` `a_planner_proposal_with_no_data_episode_field_still_supersedes_via_meta_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12270-12397` `a_same_id_refine_restamps_its_episode_so_its_own_episodes_sibling_does_not_reap_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12400-12522` `a_same_id_refine_survives_its_own_episodes_sibling_add_walked_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12525-12680` `a_resume_catch_up_over_two_episode_supersession_and_a_split_matches_a_live_incremental_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12559-12577` `append_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12579-12591` `shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12683-12854` `a_legacy_history_resume_catch_up_matches_a_live_incremental_fold_mutual_siblings_then_superseded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12704-12721` `append_legacy` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12723-12741` `append_identified` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12743-12755` `shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12857-12951` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:12954-13053` `a_late_re_emit_never_mutates_a_started_or_terminal_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13059-13068` `has_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13071-13127` `a_verbatim_copy_still_supersedes_its_baseline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13130-13204` `a_paraphrased_proposal_matches_its_baseline_by_id_not_prose` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13207-13291` `a_bracketed_id_echo_with_a_paraphrase_still_supersedes_exactly_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13294-13366` `a_stale_non_matching_id_falls_back_to_the_verbatim_prose_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13369-13427` `a_genuinely_new_proposal_runs_and_records_an_unmatched_signal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13430-13561` `resume_dedups_baselines_before_running_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13564-13645` `decomposes_the_real_spec_01_into_per_criterion_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13648-13686` `ratchet_promotes_a_reliable_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13689-13750` `elevated_gate_is_never_promoted_to_silent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13753-13795` `learns_from_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13798-13848` `feeds_graph_decisions_into_the_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13851-13930` `lookup_pointer_names_all_three_verbs_on_every_slice` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:13933-14028` `decisions_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14033-14055` `render_capped_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14058-14088` `a_superseded_decision_never_outranks_a_current_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14091-14144` `grounding_still_surfaces_a_prior_run_decision_that_peers_labels_historical` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14147-14174` `the_verbatim_count_cap_binds_on_many_small_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14177-14209` `the_byte_budget_cap_binds_before_the_count_on_chunky_decisions` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14212-14279` `a_kept_decision_restores_the_dropped_dependency_it_supersedes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14285-14305` `render_capped_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14308-14375` `findings_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14378-14413` `the_verbatim_count_cap_binds_on_many_small_findings` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14416-14521` `grounding_omits_a_resolved_finding_and_keeps_the_open_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14527-14546` `render_capped_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14549-14606` `the_injected_slice_is_deduplicated_by_normalized_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14609-14751` `dedup_and_restore_are_render_only_no_event_no_projection_mutation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14769-14905` `structural_grounding_is_one_seeded_traversal_over_the_unified_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:14922-15037` `the_grounding_path_populates_the_unified_graph_from_the_live_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15052-15141` `re_ingesting_re_extracts_a_changed_file_and_skips_unchanged_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15162-15242` `re_excluding_the_same_file_twice_in_one_process_retires_its_middle_generation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15255-15374` `a_second_run_over_an_unchanged_tree_appends_no_derived_index_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15405-15411` `spec60_content_identity` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15416-15420` `spec60_guarded_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15433-15461` `spec60_guard_is_judging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15469-15487` `spec60_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15499-15530` `spec60_reachable` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15536-15545` `spec60_reached_entities` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15562-15754` `editing_one_file_between_runs_re_emits_only_that_files_batch_and_supersedes_its_edges` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15770-15947` `a_file_reverted_to_an_earlier_recorded_generation_re_ingests_and_matches_a_cold_rebuild` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:15971-16104` `a_prior_runs_non_ingest_replay_key_never_suppresses_this_runs_keyed_emit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16116-16259` `the_ingest_sink_appends_and_folds_once_per_file_batch_never_once_per_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16126-16134` `append` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16135-16142` `read_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16143-16150` `read_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16151-16157` `subscribe_all` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16158-16164` `subscribe_stream` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16176-16179` `apply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16180-16183` `apply_batch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16184-16186` `subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16187-16189` `resolve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16282-16441` `design_intent_grounding_renders_the_governing_rule_and_specifying_ra_by_traversal` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16444-16514` `a_governing_decision_never_leaks_into_the_design_intent_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16517-16573` `the_design_intent_section_renders_the_newest_binding_and_elides_the_oldest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16576-16616` `a_subgraph_with_no_design_intent_renders_no_design_intent_header` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16623-16649` `render_code_neighborhood` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16652-16688` `the_code_neighborhood_elision_note_names_graph_around_recovery_not_peers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16691-16736` `the_code_neighborhood_byte_cap_elides_before_the_count_cap_is_reached` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16739-16769` `a_subgraph_with_no_code_definitions_renders_no_code_neighborhood_header` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16772-16838` `lessons_prompt_injection_is_capped_under_budget_with_elision_note` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16841-16878` `the_verbatim_count_cap_binds_on_many_small_lessons` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16884-16906` `render_capped_lessons_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16909-16946` `lessons_rank_by_blast_radius_relevance_over_pure_recency` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16949-16989` `resume_skips_already_integrated_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:16993-17010` `seed_events_in_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17013-17067` `replay_trajectory_keeps_only_the_world_inputs_and_restrips_the_run_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17072-17089` `commit_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17092-17187` `resume_reuses_a_units_branch_instead_of_reimplementing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17190-17284` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17292-17308` `commit_on_named_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17323-17379` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17382-17447` `branch_gc_reclaims_the_recorded_branch_not_the_derived_name_when_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17458-17482` `seed_lingering_worktree_on_unit_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17488-17498` `worktree_registered_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17501-17598` `branch_gc_removes_a_lingering_worktree_before_reclaiming_the_branch_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17601-17693` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17696-17777` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17780-17847` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17850-17949` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:17952-18035` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18038-18141` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18145-18167` `sha_stamp_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18170-18221` `review_boundary_events_carry_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18224-18273` `a_review_reject_unitfailed_carries_the_worktree_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18276-18320` `an_exhaustive_gate_failure_on_an_approved_unit_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18323-18448` `stamps_the_model_alias_and_resolved_id_on_live_lifecycle_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18454-18474` `degenerate_reviewer_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18476-18481` `has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18484-18553` `a_degenerate_adjudicator_result_respawns_and_a_substantive_retry_folds_normally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18556-18664` `a_review_stage_error_result_re_parks_a_fresh_attempt_no_charge_then_a_real_verdict_folds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18667-18798` `a_review_spawn_that_errors_on_every_re_parked_attempt_escalates_through_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18801-18894` `a_gating_spawn_that_emits_an_approve_verdict_but_returns_no_verdict_line_hard_errors_with_the_result_channel_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18897-18942` `a_gating_spawn_that_returns_a_reject_verdict_line_is_a_normal_reject_even_if_it_emitted_approve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18945-18989` `a_gating_spawn_with_no_verdict_line_and_no_emitted_approve_is_an_ordinary_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:18992-19093` `the_workflow_live_path_correlates_the_approve_by_stamp_not_a_bare_stream_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19096-19149` `a_degenerate_lens_result_respawns_the_lens_before_the_review_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19152-19212` `a_lens_that_emitted_a_finding_but_reports_empty_stdout_is_not_degenerate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19215-19312` `a_reviewer_that_only_ever_returns_degenerate_output_halts_the_run_loudly_naming_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19315-19400` `resume_integrates_an_already_approved_unit_without_re_reviewing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19403-19514` `a_failed_unit_is_not_terminal_and_resumes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19517-19632` `remediation_attempts_accumulate_across_resume_and_escalate_at_the_bound` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19635-19720` `an_escalated_unit_stays_terminal_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19723-19778` `a_fresh_unit_with_no_branch_runs_the_full_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19781-19836` `agent_decision_folds_content_but_no_agent_attribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19839-19881` `scope_creep_refuses_a_criterionless_proposed_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19884-19917` `adjudicator_reject_blocks_the_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19920-19986` `adversary_runs_between_the_lenses_and_the_adjudicator` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:19989-20039` `adjudicator_reject_gates_even_with_an_adversary_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20042-20135` `unit_reviews_itself_within_its_own_lifecycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20138-20148` `depth_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20152-20159` `full_panel_with_tiers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20161-20163` `strs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20166-20192` `path_is_high_risk_matches_by_prefix_and_by_glob` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20195-20204` `route_review_tier_with_no_policy_is_the_full_panel_unchanged` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20207-20226` `route_review_tier_routes_a_low_risk_unit_to_the_light_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20229-20244` `route_review_tier_forces_full_on_a_high_risk_path_hit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20247-20260` `route_review_tier_forces_full_over_the_size_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20263-20276` `route_review_tier_forces_full_when_the_gates_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20279-20313` `route_review_tier_fails_safe_to_full_on_an_empty_blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20319-20371` `run_tiered_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20375-20386` `logged_review_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20389-20415` `a_low_risk_unit_runs_the_light_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20418-20478` `a_low_risk_unit_skips_the_adversary_and_extra_lens` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20481-20551` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20554-20569` `without_a_depth_policy_a_grounded_unit_runs_the_full_panel_and_logs_no_routing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20572-20650` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20653-20762` `a_flapped_unit_escalates_from_light_at_attempt_0_to_full_on_remediation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20765-20829` `a_stage_level_tiers_policy_routes_the_unit_by_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20832-20914` `every_spawn_runs_in_a_worktree_never_the_main_repo_checkout` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20918-20941` `spec_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:20944-21036` `speculation_width_2_runs_two_candidates_first_green_wins_rest_cancelled` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21039-21075` `speculation_budget_accounts_for_every_candidate_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21078-21119` `speculation_defaults_off_runs_a_single_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21122-21162` `speculation_parks_all_candidates_together_in_one_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21165-21287` `speculation_defers_green_until_the_winner_and_integrates_across_replay_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21293-21303` `unit_has_status` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21307-21320` `branch_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21323-21397` `speculation_later_candidate_wins_when_an_earlier_lane_is_review_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21400-21518` `speculation_low_risk_later_lane_winner_routes_light_not_falsely_flapped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21521-21577` `speculation_rejected_loser_is_visible_to_the_review_quality_fold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21580-21647` `speculation_escalates_when_every_candidate_is_rejected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21650-21715` `speculation_crashed_lane_is_absent_and_a_sibling_still_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21718-21785` `speculation_exhaustive_integrate_door_red_blocks_a_candidate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21788-21895` `speculation_stays_fresh_across_parking_until_a_winner_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21910-21977` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:21981-22147` `speculation_blocked_winner_captures_evidence_and_a_later_lane_wins` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22150-22204` `assert_isolated_cwd_refuses_empty_or_repo_root_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22207-22298` `the_conductor_threads_each_agents_persona_to_the_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22301-22355` `the_system_prompt_carries_the_rigger_communication_discipline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22358-22398` `an_agent_with_no_persona_threads_an_empty_system_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22401-22463` `planner_proposed_unit_inherits_the_default_review_panel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22466-22561` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22564-22636` `per_unit_adjudicator_reject_blocks_integration_and_escalates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22639-22732` `an_always_rejecting_adjudicator_escalates_after_exactly_max_retries_cycles` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22735-22832` `a_higher_max_retries_gives_more_attempts_before_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22749-22795` `escalation_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22835-22899` `an_absent_max_retries_preserves_the_default_bound_of_three` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22917-22923` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22926-22959` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:22963-23036` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23039-23129` `a_model_ladder_implementer_escalates_one_rung_per_remediation_attempt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23132-23198` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23201-23235` `mid_spawn_crash_escalates_without_aborting_the_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23238-23289` `a_newly_escalated_unit_stamps_an_attention_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23292-23342` `a_budget_halt_stamps_an_attention_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23345-23434` `a_budget_halt_does_not_restamp_on_a_later_poll_with_nothing_new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23437-23541` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23544-23615` `compute_attention_leaves_the_hung_liveness_halt_to_the_caller` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23618-23699` `an_escalation_does_not_restamp_attention_on_a_resumed_process` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23702-23735` `a_clean_step_stamps_no_attention` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23738-23862` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23865-23912` `nine_of_ten_budget_crosses_the_final_tenth` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:23915-24002` `a_re_step_already_past_the_budget_threshold_does_not_re_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24005-24056` `budget_breaker_stops_the_run_after_the_first_wave` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24059-24102` `budget_exhaustion_aborts_the_task` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24105-24143` `a_budget_halt_surfaces_its_reason_on_the_run_state` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24146-24178` `a_run_within_budget_surfaces_no_halt_reason` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24181-24249` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24252-24293` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24296-24366` `a_review_tier_budget_refusal_aborts_with_budgetexhausted_not_a_raw_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24369-24401` `coverage_gap_flags_a_spec_defect_and_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24404-24445` `planner_covering_every_criterion_passes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24448-24482` `planner_leaving_a_gap_flags_a_spec_defect` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24485-24519` `gate_only_stage_is_a_coverage_proxy_gap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24522-24581` `manual_stage_pauses_while_an_auto_stage_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24584-24627` `isolation_none_agent_gets_no_worktree_even_with_a_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24630-24663` `spawn_opts_isolation_is_set_for_a_worktree_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24666-24705` `a_spawned_implementers_title_is_the_unit_criterion` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24712-24760` `the_adversarys_spawn_is_stamped_with_the_units_lens_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24766-24815` `the_adjudicators_spawn_is_stamped_with_lenses_plus_adversary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24820-24867` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24876-24939` `the_roster_reflects_the_actually_routed_light_panel_not_the_full_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:24953-25010` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25013-25079` `stage_autonomy_override_seeds_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25082-25129` `on_pass_none_runs_gates_but_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25140-25155` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25159-25246` `two_units_gate_environments_never_share_a_target_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25255-25259` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25260-25262` `envs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25265-25274` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25282-25409` `one_build_environment_authority_reaches_both_a_gate_build_and_an_agent_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25297-25334` `run_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25412-25456` `spawn_env_adds_the_per_unit_cargo_target_dir_only_for_a_real_unit_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25459-25522` `a_units_per_unit_cache_is_reclaimed_when_its_worktree_is_removed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25525-25597` `a_units_worktree_cache_and_branch_are_all_reclaimed_on_a_successful_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25600-25660` `a_units_worktree_is_reclaimed_but_its_branch_survives_a_terminal_escalation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25663-25769` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25772-25864` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25867-25969` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:25972-26070` `verified_worktree_sha_is_stamped_after_a_gate_side_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26073-26151` `review_tier_boundary_restores_a_worktree_a_prior_tier_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26154-26268` `reviewed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26271-26338` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26341-26444` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26447-26532` `a_resumed_reviewed_unit_whose_exhaustive_gate_fails_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26535-26673` `a_resumed_reviewed_units_merge_break_records_an_integrate_conflict_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26676-26750` `a_live_approved_unit_restores_a_worktree_the_integrate_door_exhaustive_gate_deleted` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26753-26870` `speculation_lenses_restore_a_gate_deleted_worktree_without_racing_and_stamp_the_winner_sha` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26873-26944` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:26947-27067` `a_parked_lens_keeps_the_unit_worktree_beside_a_genuine_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27070-27115` `a_worktree_less_stage_gate_inherits_the_shared_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27118-27155` `bounded_pool_completes_every_stage_under_the_cap` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27158-27204` `live_run_folds_no_gate_machinery` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27207-27250` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27253-27294` `standalone_review_stage_still_takes_the_fan_out_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27297-27409` `run_gates_derives_and_injects_the_review_worktrees_store_fence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27412-27465` `a_parked_lens_keeps_the_standalone_review_stages_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27468-27564` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27567-27660` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27663-27748` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27751-27838` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27841-27908` `a_budget_refused_standalone_review_spawn_keeps_its_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27911-27938` `partition_separates_overlapping_blast_radii` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27941-27963` `blast_radius_conflicts_flags_units_that_split_one_file_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27966-27979` `blast_radius_conflicts_is_empty_for_a_disjoint_partition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:27982-28030` `partitioned_wave_still_integrates_every_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28042-28065` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28069-28178` `lens_finding_reaches_later_tiers_through_the_graph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28181-28242` `review_agents_emit_findings_via_the_review_protocol` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28245-28290` `unparseable_adjudicator_output_blocks_integration` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28293-28354` `failing_gate_evidence_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28357-28440` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28443-28513` `a_product_gate_failure_is_not_rerun_and_demotes_as_before` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28516-28594` `an_authored_infra_rule_with_limit_zero_at_an_inline_gate_holds_the_ratchet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28597-28692` `an_inline_gate_red_across_every_rerun_holds_for_infra_and_demotes_for_flaky` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28695-28734` `unit_evidence_is_populated_after_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28737-28812` `adjudicator_rejection_reasoning_threaded_into_retry_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28815-28882` `fan_out_lens_does_not_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28885-28937` `review_only_stage_records_no_artifact_truthfully` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:28940-28998` `two_erroring_stages_both_leave_a_record` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29001-29065` `single_wide_wave_overruns_budget_and_is_stopped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29068-29114` `validate_acyclic_detects_a_cycle` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29161-29173` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29176-29181` `materializing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29185-29190` `deleting_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29191-29193` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29194-29196` `targets` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29197-29199` `build_envs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29200-29202` `store_fences` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29203-29205` `build_cache_guards` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29206-29208` `build_cache_dirs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29211-29256` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29261-29283` `content_cache_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29287-29293` `attempt_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29305-29342` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29353-29358` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29359-29361` `calls` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29364-29383` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29387-29396` `gate_verdict_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29398-29402` `verdict_passed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29405-29471` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29474-29516` `a_content_change_misses_the_cache_and_re_runs_the_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29519-29575` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29584-29596` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29608-29623` `ground` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29624-29626` `blast_radius` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29627-29629` `index_stamp` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29633-29652` `blast_radius_audits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29655-29664` `review_tier_evidence` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29669-29692` `tiered_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29694-29701` `tiered_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29711-29784` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29791-29831` `the_empty_radius_fail_safe_still_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29839-29873` `a_non_structural_grounder_records_no_blast_radius_audit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29885-29950` `confidence_tier_fixture` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:29961-30012` `confidence_tier_radius_splits_the_one_edge_set_into_extracted_precise_and_all_tier_safe` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30022-30113` `grounded_blast_radius_tier_filters_the_subgraph_and_keeps_the_grep_superset` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30028-30030` `apply` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30031-30037` `subgraph` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30038-30040` `resolve` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30136-30229` `under_symbols_the_audit_emits_and_records_the_prompt_seed_as_precise` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30241-30315` `partition_wave_own_batches_an_empty_radius_never_co_scheduling_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30324-30388` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30391-30442` `staleness_flags_only_downstream_units_whose_radius_intersects_the_touched_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30445-30497` `staleness_grounds_on_the_safe_superset_so_a_grep_only_reference_still_stales_a_downstream_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30500-30598` `rule6_conflict_detection_grounds_on_the_safe_superset_so_a_grep_only_shared_reference_conflicts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30601-30631` `a_resume_reseeds_the_stale_set_from_the_prior_unitintegrated_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30640-30681` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30685-30810` `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30813-30839` `glob_matches_supports_star_doublestar_and_literals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30842-30866` `gate_intersects_radius_runs_unscoped_and_intersecting_gates_only` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:30869-31018` `blast_radius_narrows_the_inner_loop_and_the_integrate_step_runs_the_full_library` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31021-31149` `a_gate_skipped_inline_but_red_at_the_exhaustive_integrate_door_blocks_the_merge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31152-31181` `verdict_compensates_names_a_prior_unit_independent_of_approval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31194-31236` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31240-31415` `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31418-31478` `pending_compensations_from_log_re_derives_only_undrained_marks` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31481-31512` `a_durable_compensation_queued_mark_is_fold_neutral_on_the_target` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31520-31557` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31561-31644` `a_compensated_unit_re_gates_its_re_implemented_tree_not_the_condemned_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31655-31700` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31704-31802` `a_compensated_unit_that_remediated_before_integrating_re_gates_at_a_fresh_key` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31812-31836` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31840-31975` `a_resume_re_drives_a_durably_queued_but_undrained_compensation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:31995-32043` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32047-32229` `integrate_re_gates_the_merged_tree_and_a_merge_break_blocks_the_second_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32232-32306` `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32309-32377` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32388-32410` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32414-32485` `a_default_infra_fault_at_a_deferred_gate_does_not_demote` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32488-32592` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32595-32680` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32683-32827` `a_parked_step_never_records_a_partial_tree_deferred_verdict` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32830-32914` `a_manual_review_paused_unit_defers_the_whole_tree_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:32917-33050` `an_escalated_dep_still_runs_the_whole_tree_deferred_gate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33053-33167` `a_recorded_failing_deferred_verdict_re_surfaces_its_failure_on_replay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33170-33256` `a_replayed_fan_out_review_reject_appends_no_duplicate_unitfailed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33259-33307` `a_fan_out_review_stage_whose_gates_fail_after_approval_records_a_gate_cause` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33311-33319` `git_head` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33321-33338` `init_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33349-33379` `run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33383-33442` `gate_measures_the_committed_artifact_not_the_dirty_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33445-33571` `conductor_spawns_the_sdet_author_at_the_build_seam_so_its_tests_land_in_the_committed_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33574-33664` `the_sdet_author_spawn_respects_the_budget_breaker_at_its_own_spawn_site` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33667-33769` `a_parked_sdet_author_spawn_holds_the_unit_instead_of_integrating_without_its_periphery_tests` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33772-33884` `speculation_sdet_author_periphery_lands_in_the_committed_tree_the_gates_judge` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33887-33984` `a_parked_sdet_author_in_a_speculation_candidate_holds_the_unit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:33987-34062` `an_empty_accounting_sdet_author_result_advances_the_build_to_commit_gates_and_integrate` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34065-34129` `an_absent_sdet_author_is_a_clean_no_op_and_the_build_still_integrates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34132-34207` `a_crashed_sdet_author_spawn_does_not_block_the_build_the_lifecycle_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34210-34310` `an_escalated_unit_does_not_integrate_its_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34318-34357` `critique_cfg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34386-34397` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34400-34405` `rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34408-34413` `always_rejecting` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34414-34421` `count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34424-34474` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34478-34547` `a_blast_radius_overlap_alone_does_not_reject` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34560-34604` `the_plan_critique_gates_adversary_and_adjudicator_spawns_are_stamped_correctly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34607-34644` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34647-34698` `a_clean_decomposition_approves_and_releases_the_fan_out` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34701-34776` `the_gate_critiques_only_not_yet_run_units_and_approves` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34779-34816` `the_plan_critique_prompt_names_the_cross_unit_rules` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34819-34843` `the_critique_gate_never_enters_a_wave_even_when_ready` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34846-34920` `a_resolved_plan_critique_gate_does_not_re_run_on_resume` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:34923-35021` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35024-35080` `an_approved_gate_releases_planner_proposed_units_not_only_baselines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35083-35150` `the_re_plan_directive_instructs_reusing_the_existing_unit_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35163-35168` `new` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35169-35176` `count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35179-35199` `spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35203-35274` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35277-35331` `a_parked_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35334-35384` `a_budget_refused_plan_critique_gate_keeps_its_review_worktree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/conductor.rs:35397-35424` `the_conductors_one_event_authority_reports_a_write_the_store_lost` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
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
- `main::commands` (34 functions)
  - `src/main.rs:270-273` `cmd_version` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2000-2008` `cmd_run` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2095-2601` `cmd_step` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2831-2863` `cmd_reported` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2881-2897` `cmd_prompt` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:2921-2952` `cmd_scratch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3532-3539` `cmd_serve` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3584-3640` `cmd_workflow` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3709-3769` `cmd_graph` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:3793-3813` `cmd_graph_show` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4020-4103` `cmd_graph_build` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4124-4183` `cmd_graph_communities` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4203-4262` `cmd_graph_concepts` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4281-4319` `cmd_stats` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:4643-4653` `cmd_stats_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5012-5135` `cmd_canary` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5145-5181` `cmd_playbooks` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:5214-5366` `cmd_replay` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:6201-6508` `cmd_dash` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:6960-6989` `cmd_ground` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7005-7026` `cmd_reindex` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7043-7071` `cmd_symbols_index` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7079-7155` `cmd_emit` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7166-7200` `cmd_progress` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7250-7408` `cmd_status` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7511-7542` `cmd_watch` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:7798-7876` `cmd_reset` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8545-8585` `cmd_peers` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:8792-8888` `cmd_result` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:9024-9194` `cmd_validate` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:11210-11229` `cmd_init` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:11851-11971` `cmd_setup` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12245-12250` `cmd_docs` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
  - `src/main.rs:12390-12424` `cmd_prime` - name contains "cmd_" (CLI command handler); grouped under `main::commands`.
- `main::dash_glue` (25 functions)
  - `src/main.rs:111-115` `record_dash_attempt` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:5711-5713` `dash_marker_serving` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:5793-5813` `spawn_dash_child_process` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:5838-5859` `wait_for_dash_bind` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:5886-5919` `wait_for_dash_bind_or_diagnose` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6072-6074` `dash_ensure_suppressed` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6081-6083` `dash_ensure_port` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6089-6092` `dash_ensure_port_from` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6153-6157` `recorded_dash_url` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6167-6181` `dash_status_line` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6190-6199` `dash_status_json` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6515-6522` `dash_reap_poll` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6531-6539` `dash_reap_idle_window` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6654-6673` `dash_read_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6679-6691` `dash_read_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6702-6710` `dash_read_whole_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6719-6736` `dash_read_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6742-6754` `dash_attach_calls` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6759-6776` `dash_read_progress` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6782-6809` `dash_read_liveness` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6832-6848` `dash_resolve_attach` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6873-6883` `dash_read_sqlite_stream_readonly` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6885-6921` `dash_attach_run` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6928-6940` `dash_attach_inputs` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
  - `src/main.rs:6946-6952` `dash_attach_graph` - name contains "dash_" (dash registry/marker glue); grouped under `main::dash_glue`.
- `main::docs_overlay` (1 function)
  - `src/main.rs:11363-11370` `apply` - method inside `impl DocsOverlay`; grouped with its other `DocsOverlay` methods.
- `main::liveness` (9 functions)
  - `src/main.rs:2655-2665` `live_branches_for_sweep` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:2696-2712` `terminal_and_no_live_worker` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:7216-7240` `liveness_ages_for_wave` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8241-8279` `live_writer_reasons` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8285-8297` `live_writer_refusal` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8461-8471` `superseded_edge_boundary` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:8493-8523` `superseded_graph_nodes` - name contains "superseded" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:9960-9967` `live_slugs` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
  - `src/main.rs:10141-10159` `worktree_belongs_to_live` - name contains "live" (liveness/heartbeat reporting); grouped under `main::liveness`.
- `main::provenance` (23 functions)
  - `src/main.rs:200-202` `workflow_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3178-3220` `refuse_when_base_lacks_spec_paths` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3407-3530` `run_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:3545-3576` `parse_workflow_args` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9017-9022` `spec_lint_warning_lines` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9418-9423` `installed_workflow_drifted` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9430-9432` `workflow_provenance_path` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9437-9445` `installed_workflow_provenance` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:9464-9475` `git_is_ancestor` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:9483-9493` `git_commit_distance` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:9620-9654` `workflow_drift_advisory` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:11296-11306` `install_workflow` - name contains "workflow" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:11335-11337` `docs_overlay_path` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:11378-11387` `read_docs_overlay` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:11686-11707` `git_hooks_dir` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:12171-12209` `docs_context` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12265-12284` `docs_drift` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12293-12309` `docs_drift_failure` - name contains "docs_" (docs overlay); grouped under `main::provenance`.
  - `src/main.rs:12331-12337` `spec_lint_next_step` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12358-12362` `spec_lint_reminder_suppressed` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12370-12382` `spec_lint_reminder_should_print` - name contains "spec_" (workflow/spec loading); grouped under `main::provenance`.
  - `src/main.rs:12465-12468` `git_repo` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
  - `src/main.rs:12476-12486` `git_repo_at` - name contains "git_" (git/version provenance); grouped under `main::provenance`.
- `main::render` (8 functions)
  - `src/main.rs:3819-3856` `print_entity_site` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4416-4458` `format_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4504-4520` `format_progress_line` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:4682-4778` `format_canary_stats` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:5562-5572` `format_stats_diff` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:10374-10404` `format_residue` - name contains "format_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:11056-11068` `print_orientation` - name contains "print_" (human-readable output); grouped under `main::render`.
  - `src/main.rs:12488-12507` `print_run_state` - name contains "print_" (human-readable output); grouped under `main::render`.
- `main::replay_runner` (1 function)
  - `src/main.rs:5583-5602` `run` - method inside `impl Runner for ReplayRunner`; grouped with its other `ReplayRunner` methods.
- `main::residue_report` (1 function)
  - `src/main.rs:9731-9736` `is_empty` - method inside `impl ResidueReport`; grouped with its other `ResidueReport` methods.
- `main::run_registration` (2 functions)
  - `src/main.rs:760-765` `inert` - method inside `impl RunRegistration`; grouped with its other `RunRegistration` methods.
  - `src/main.rs:769-776` `drop` - method inside `impl Drop for RunRegistration`; grouped with its other `RunRegistration` methods.
- `main::scaffold_report` (1 function)
  - `src/main.rs:10912-10918` `changed` - method inside `impl ScaffoldReport`; grouped with its other `ScaffoldReport` methods.
- `main::setup` (18 functions)
  - `src/main.rs:1812-1815` `scratch_defaults` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:3655-3673` `locate_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:6558-6562` `foreign_instance_scratch_root` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:10475-10495` `scratch_totals` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:10657-10692` `classify_agent_scratch` - name contains "scratch" (project setup); grouped under `main::setup`.
  - `src/main.rs:11040-11047` `print_scaffold_pointer` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:11177-11208` `scaffold_summary_lines` - name contains "scaffold" (project setup); grouped under `main::setup`.
  - `src/main.rs:11235-11237` `shim_dir` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:11265-11282` `install_file_if_changed` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:11314-11316` `skill_source_rel` - name contains "skill" (project setup); grouped under `main::setup`.
  - `src/main.rs:11325-11330` `skill_install_path` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:11400-11413` `install_skills` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:11719-11747` `install_precommit_hook` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:11764-11772` `provision_shim` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:11786-11795` `shim_is_current` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:11800-11807` `write_shim_files` - name contains "shim" (project setup); grouped under `main::setup`.
  - `src/main.rs:11814-11842` `run_npm_install` - name contains "install" (project setup); grouped under `main::setup`.
  - `src/main.rs:11983-11999` `parse_setup_args` - name contains "setup" (project setup); grouped under `main::setup`.
- `main::store` (32 functions)
  - `src/main.rs:473-491` `store_conn_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:548-561` `store_backend_kind` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:611-677` `store_selection_at` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:685-691` `store_selection` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:719-732` `registry_store_identity` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1241-1312` `migrate_project_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1321-1326` `migrate_local_identity` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1338-1364` `migrate_identity_at` - name contains "migrate" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1703-1705` `find_store_dir_from` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1826-1831` `server_store_location` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1851-1917` `require_store_dir` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:1921-1923` `store_file` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:2758-2764` `reclaim_run_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:7895-7921` `reset_menu` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:7986-8022` `reset_modes` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8044-8063` `reset_build_cache` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8068-8081` `build_cache_reclaim_report` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8109-8117` `reset_derived` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8328-8389` `refuse_derived_reset_if_live` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8416-8447` `reset_runs` - name contains "reset_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8921-8936` `reclaim_spawn_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:8963-8980` `reclaim_spawn_registered_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9341-9351` `bloat_advisory` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9364-9375` `bloat_advisory_for` - name contains "bloat" (store hygiene); grouped under `main::store`.
  - `src/main.rs:9989-10037` `reclaim_orphan_scratch` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10312-10353` `reclaim_shared_build_cache` - name contains "reclaim_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10512-10555` `scratch_footprint` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10565-10588` `store_and_backup_bytes` - name contains "store_" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10709-10786` `footprint_report` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10792-10797` `footprint_report_lines` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10806-10826` `footprint_advisories` - name contains "footprint" (store hygiene); grouped under `main::store`.
  - `src/main.rs:10857-10884` `footprint_report_for` - name contains "footprint" (store hygiene); grouped under `main::store`.
- `main::store_location` (3 functions)
  - `src/main.rs:1755-1757` `file` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1763-1770` `identity` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
  - `src/main.rs:1783-1789` `repo_root` - method inside `impl StoreLocation`; grouped with its other `StoreLocation` methods.
- `main::store_selection` (1 function)
  - `src/main.rs:434-436` `is_sqlite` - method inside `impl StoreSelection`; grouped with its other `StoreSelection` methods.
- `main::support` (25 functions)
  - `src/main.rs:321-397` `parse_run_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:408-416` `resolve_run_base` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:498-500` `conn_file_is_group_or_other_readable` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:508-530` `warn_if_conn_file_is_exposed` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:568-591` `resolve_conn` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:699-709` `resolve_store` - name contains "resolve" (path/id resolution helper); grouped under `main::support`.
  - `src/main.rs:979-988` `read_project_id` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:3010-3053` `parse_step_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:4788-4800` `read_model_drift` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:4886-4898` `read_order_signatures` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:4922-4994` `parse_canary_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:5443-5475` `parse_replay_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:5726-5752` `ensure_run_dashboard_at` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:6112-6147` `ensure_run_dashboard` - name contains "ensure_" (invariant helper); grouped under `main::support`.
  - `src/main.rs:7454-7484` `parse_watch_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:8656-8714` `parse_result_args` - name contains "parse_" (argument/input parsing); grouped under `main::support`.
  - `src/main.rs:8754-8768` `read_outcome_from_stdin` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:9836-9864` `read_run_units` - name contains "read_" (generic read helper); grouped under `main::support`.
  - `src/main.rs:10163-10165` `is_uuid8` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:10172-10201` `find_shadow_stores` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:10264-10269` `is_build_cache_tombstone` - name contains "is_" (predicate helper); grouped under `main::support`.
  - `src/main.rs:11087-11136` `write_gitignore_entries` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:11588-11593` `find_bytes` - name contains "find_" (lookup helper); grouped under `main::support`.
  - `src/main.rs:12219-12237` `write_docs` - name contains "write_" (generic write helper); grouped under `main::support`.
  - `src/main.rs:12517-12524` `write_if_absent` - name contains "write_" (generic write helper); grouped under `main::support`.
- `main::tests` (336 functions)
  - `src/main.rs:11675-11680` `compose_precommit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12706-12713` `spec_lint_next_step_names_rigger_validate_and_the_given_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12721-12723` `spec_lint_reminder_suppressed_when_env_names_the_real_direct_parent_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12729-12750` `spec_lint_reminder_prints_on_absent_foreign_stale_or_malformed_env` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12767-12789` `spec_lint_warning_lines_formats_every_advisory_with_the_spec_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12794-12797` `spec_lint_warning_lines_is_empty_on_a_clean_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12807-12860` `ensure_run_dashboard_at_starts_once_then_short_circuits_on_a_live_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12869-12875` `dash_marker_serving_reports_false_when_nothing_answers_the_markers_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12880-12912` `ensure_run_dashboard_at_restarts_when_the_recorded_dash_is_gone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12920-12939` `ensure_run_dashboard_at_reports_failed_when_the_start_errors` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12951-12990` `wait_for_dash_bind_confirms_promptly_once_the_port_answers_as_a_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:12997-13024` `wait_for_dash_bind_fails_fast_when_the_child_exits_before_confirming` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13038-13080` `wait_for_dash_bind_times_out_against_a_real_held_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13103-13147` `wait_for_dash_bind_or_diagnose_never_self_attributes_its_own_still_starting_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13164-13192` `ensure_run_dashboard_at_writes_no_marker_when_the_real_spawn_never_confirms_a_bind` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13207-13252` `ensure_run_dashboard_at_names_the_held_ports_holder_when_the_real_spawn_fails` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13272-13296` `ensure_run_dashboard_at_never_claims_a_phantom_holder_for_an_unheld_port` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13308-13339` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_dead_pid` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13349-13383` `ensure_run_dashboard_at_self_heals_a_marker_naming_a_live_pid_whose_port_is_unserved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13396-13434` `ensure_run_dashboard_at_never_revisits_a_still_serving_unattributed_pid_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13441-13462` `dash_ensure_is_suppressed_by_either_opt_out_and_proceeds_when_neither_is_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13472-13503` `dash_status_line_renders_each_outcome_to_its_exact_text` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13511-13539` `dash_status_json_renders_each_outcome_to_its_exact_shape` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13548-13575` `dash_ensure_port_defaults_to_the_fixed_address_and_only_a_valid_override_relocates_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13585-13618` `compose_precommit_fresh_install_carries_the_managed_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13628-13655` `precommit_block_refuses_on_drift_instead_of_staging` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13669-13770` `precommit_block_resolves_a_tree_built_binary_before_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13777-13789` `compose_precommit_is_idempotent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13800-13829` `compose_precommit_chains_without_clobbering_an_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13839-13853` `compose_precommit_prepends_before_a_terminal_exit_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13862-13913` `install_precommit_hook_preserves_a_non_utf8_existing_hook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13920-13943` `compose_precommit_refreshes_a_stale_block_in_place` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13951-13970` `cmd_peers_prints_live_or_historical_per_decision_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13988-14031` `superseded_graph_nodes_drops_dead_runs_and_preboundary_keeping_lessons_active_and_reused_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13990-13992` `ev` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:13993-13998` `run_started` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14040-14078` `superseded_edge_boundary_is_the_active_runs_start_or_none_without_a_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14042-14048` `run_started_at` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14049-14055` `decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14106-14364` `the_denoise_leaves_metrics_run_pruning_and_blast_radius_unaffected` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14112-14116` `ev` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14374-14392` `version_line_carries_the_derived_version_and_a_non_empty_build_provenance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14399-14440` `docs_context_reads_every_fact_from_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14447-14477` `docs_render_surfaces_known_code_facts_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14483-14502` `commands_registry_is_well_formed_and_covers_dispatch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14510-14540` `write_docs_writes_every_registry_skill_plus_the_handbook` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14550-14587` `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14596-14635` `per_operation_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14645-14673` `watching_discipline_skills_reference_only_real_subcommands` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14681-14748` `docs_drift_flags_a_changed_file_and_skips_absent_or_in_sync_ones` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14759-14785` `committed_registry_docs_are_in_sync_with_a_fresh_render` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14796-14825` `planning_field_guide_page_renders_and_is_linked_from_authoring_loops` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14836-14903` `status_and_dashboard_render_the_same_current_blocker_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14916-14971` `release_ready_lines_surface_only_on_a_done_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:14974-15040` `status_and_dash_read_the_runs_persisted_base_not_a_re_resolution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15045-15066` `dirty_tracked_paths_keeps_tracked_modifications_and_drops_untracked_and_ignored` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15069-15071` `dirty_tracked_paths_on_a_clean_tree_is_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15074-15099` `installed_workflow_drifted_is_false_when_absent_or_identical_and_true_on_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15104-15151` `drift_side_names_the_binary_stale_only_when_the_installed_workflow_is_provably_newer` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15154-15202` `workflow_drift_advisory_names_which_side_is_stale_and_never_says_they_differ` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15205-15249` `git_is_ancestor_decides_commit_order_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15252-15298` `git_commit_distance_counts_commits_ahead_in_a_real_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15301-15315` `missing_gitsemver_binary_advisory_fires_only_on_the_unversioned_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15318-15323` `behind_the_tree_message_is_silent_when_versions_already_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15326-15336` `behind_the_tree_message_is_silent_when_either_side_is_unversioned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15339-15348` `behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15351-15358` `behind_the_tree_message_names_both_versions_and_the_commit_distance` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15365-15371` `gitsemver_available` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15375-15390` `behind_the_tree_git` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15392-15404` `behind_the_tree_git_output` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15407-15445` `behind_the_tree_advisory_names_the_real_derived_version_ahead_of_the_installed_commit` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15448-15470` `behind_the_tree_advisory_is_silent_when_the_checkout_has_not_moved` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15473-15486` `install_workflow_records_the_build_provenance_beside_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15489-15504` `validate_advisories_warns_on_workflow_drift_naming_the_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15510-15512` `slugs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15514-15517` `write_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15522-15550` `init_committed_repo` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15553-15577` `refuse_when_base_lacks_spec_paths_refuses_on_total_absence_and_names_a_path` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15580-15597` `refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15600-15618` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15621-15661` `refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15664-15745` `refuse_when_base_unreachable_fails_loudly_only_when_no_reachable_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15748-15755` `human_size_formats_bytes_through_gib` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15758-15764` `is_uuid8_accepts_exactly_eight_hex_digits` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15767-15812` `worktree_belongs_to_live_matches_both_naming_shapes_without_prefix_false_match` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15815-15862` `current_run_units_scopes_to_the_current_run_and_splits_live_from_dead` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15865-15902` `current_run_units_spares_a_terminal_units_branch_whose_latest_spawn_is_still_in_flight` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15905-15931` `current_run_units_still_retires_a_terminal_unit_once_its_latest_spawn_answers` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15934-15981` `current_run_units_splits_live_spawns_from_answered_ones_scoped_to_the_current_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:15993-16019` `live_branches_for_sweep_fails_closed_on_an_unreadable_stream_but_folds_a_readable_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16022-16062` `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16065-16076` `dir_size_bytes_sums_files_recursively` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16081-16097` `reclaim_shared_build_cache_deletes_a_populated_cache_and_reports_its_bytes` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16100-16118` `reclaim_shared_build_cache_is_idempotent_zero_report_when_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16121-16149` `reclaim_shared_build_cache_refuses_rather_than_waits_when_a_build_holds_the_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16152-16175` `reclaim_shared_build_cache_releases_the_guard_promptly_after_a_successful_reclaim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16178-16251` `scan_residue_reports_dead_worktrees_caches_shadows_and_branches` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16254-16273` `scan_residue_is_empty_when_everything_is_live_and_no_shadow_stores` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16276-16293` `format_residue_renders_a_sized_warning_block` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16298-16317` `store_and_backup_bytes_splits_live_store_files_from_dot_bak_backups` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16320-16324` `store_and_backup_bytes_is_zero_on_a_missing_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16327-16369` `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16372-16437` `footprint_report_measures_every_category_on_a_seeded_fixture_tree` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16440-16457` `footprint_report_folds_a_none_mutation_root_to_a_zero_contribution` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16460-16542` `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16555-16609` `footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16612-16617` `looks_like_run_container_true_when_every_direct_child_is_a_directory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16620-16628` `looks_like_run_container_false_when_a_direct_child_is_a_bare_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16631-16639` `looks_like_run_container_is_true_on_an_empty_or_missing_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16642-16655` `classify_agent_scratch_separates_a_bare_top_level_file_from_a_well_formed_container` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16658-16737` `footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16740-16764` `footprint_report_lines_reports_every_categorys_total_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16767-16778` `footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16781-16789` `footprint_advisories_is_silent_below_the_threshold` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16792-16809` `footprint_advisories_flags_a_category_exactly_at_the_threshold_boundary` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16812-16822` `footprint_advisories_never_flags_a_category_with_no_reclaim_command` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16825-16833` `footprint_advisories_is_silent_on_an_empty_category` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16836-16859` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16864-16977` `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:16980-17005` `reclaim_orphan_scratch_reaps_a_stray_build_cache_tombstone_unconditionally` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17010-17049` `leaked_process_advisories_name_a_process_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17052-17062` `leaked_process_advisories_is_empty_when_no_process_is_rooted_under_the_scratch_root` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17065-17072` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17077-17083` `parse_result_takes_an_id_and_an_optional_output_arg` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17086-17091` `parse_result_with_no_output_defers_to_stdin` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17096-17102` `find_store_dir_from_returns_the_dir_that_holds_the_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17105-17117` `find_store_dir_from_walks_up_from_a_subdirectory` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17120-17126` `git_init_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17129-17166` `find_store_dir_from_never_escapes_the_repo_into_a_parent_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17169-17244` `reap_then_remove_dir_reaps_processes_rooted_inside_then_removes_the_dir` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17247-17296` `reclaim_run_scratch_removes_the_run_level_areas_and_spares_per_unit_scratch` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17299-17332` `reclaim_run_scratch_spares_the_shared_build_cache_while_a_build_holds_its_guard` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17335-17346` `find_store_dir_from_refuses_the_worktree_shape_with_no_events_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17349-17378` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17381-17408` `walk_stores_from_prefers_the_outermost_store_over_a_nearer_shadow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17411-17429` `walk_stores_from_reports_no_shadow_for_a_single_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17438-17507` `require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17441-17444` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17511-17533` `require_store_dir_fence_is_off_by_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17517-17519` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17538-17544` `result_advisories_flags_an_orphan_id_with_no_spawn_request` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17547-17572` `result_advisories_orphan_wording_is_plain_on_record_and_conditional_under_if_absent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17575-17585` `result_advisories_is_silent_for_a_parked_unanswered_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17588-17599` `result_advisories_flags_a_supersede_with_the_prior_result_position` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17602-17615` `result_advisories_suppresses_the_supersede_note_when_not_superseding` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17618-17630` `result_advisories_flags_both_orphan_and_supersede` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17633-17653` `parse_result_error_flag_is_order_independent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17656-17683` `parse_result_if_absent_is_off_by_default_and_a_bare_order_independent_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17686-17721` `parse_result_meta_must_be_a_json_object` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17724-17738` `parse_result_rejects_missing_id_extra_args_and_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17741-17755` `build_result_shapes_success_and_failure` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17758-17763` `build_result_rejects_a_blank_error_message` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17766-17775` `build_result_attaches_meta` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17778-17816` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17819-17851` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17857-17924` `scaffold_parses_into_a_valid_config` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17938-17955` `shipped_workflows_carry_a_non_zero_spawn_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17958-17970` `parse_canary_args_defaults_corpus_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17973-17985` `parse_canary_args_reads_corpus_if_model_changed_and_jobs` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:17988-18000` `parse_canary_args_rejects_a_non_positive_jobs_value` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18003-18013` `parse_canary_args_rejects_unknown_flags` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18016-18045` `usage_text_gives_the_jobs_flag_its_own_description_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18048-18065` `usage_text_names_the_build_cache_reset_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18068-18078` `parse_run_args_defaults_to_cli_and_an_unset_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18081-18092` `parse_run_args_reads_fresh_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18095-18105` `parse_run_args_reads_rebase_definition` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18108-18123` `parse_run_args_reads_driver_eventstore_conn_and_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18126-18131` `parse_run_args_rejects_unknown_flags_and_values` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18138-18164` `parse_run_args_accepts_base_alongside_a_spec` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18172-18194` `resolve_run_base_precedence_flag_then_env_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18200-18238` `parse_workflow_args_reads_spec_and_base` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18243-18295` `parse_step_args_reads_spec_and_base_with_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18301-18345` `definition_hash_is_stable_and_content_sensitive` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18354-18381` `kurrentdb_is_always_available_and_needs_a_conn` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18393-18424` `store_selection_preserves_a_credentialed_tls_conn_verbatim` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18435-18612` `store_selection_precedence_flag_env_secret_file_config_then_default` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18615-18617` `project_identity_is_never_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18620-18645` `project_identity_reads_the_tracked_id_file_then_falls_back_to_the_basename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18648-18675` `ssh_https_and_git_suffix_forms_of_one_repo_mint_identical_ids` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18678-18692` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18695-18724` `decide_migration_covers_every_case` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18727-18766` `migrate_project_identity_renames_legacy_history_and_records_a_decision` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18769-18808` `migrate_project_identity_refuses_when_both_namespaces_hold_history` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18811-18876` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18879-18959` `migrate_project_identity_rekeys_the_graph_before_the_irreversible_stream_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:18962-19057` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19066-19171` `dash_graph_provider_reaches_the_whole_projection_when_run_seeds_are_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19185-19224` `dash_read_whole_graph_on_an_absent_db_is_empty_and_creates_nothing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19230-19262` `setup_provisions_the_shim_runtime_files` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19271-19330` `setup_installs_refreshes_and_is_a_noop_on_the_native_rigger_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19335-19344` `outcome_for` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19348-19353` `registry_entry` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19363-19433` `setup_installs_the_using_rigger_skill_distinct_from_the_workflow` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19451-19535` `planning_a_spec_installs_and_renders_through_the_registry_with_no_planning_specific_code` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19545-19581` `setup_skill_install_applies_the_project_overlay` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19587-19613` `docs_overlay_overrides_only_declared_fields` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19618-19628` `docs_overlay_malformed_is_a_loud_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19638-19666` `provision_shim_is_a_silent_noop_when_already_current` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19676-19695` `shim_is_not_current_when_node_modules_is_torn_missing_the_install_marker` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19702-19724` `init_project_is_idempotent_reporting_new_work_only_once` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19734-19792` `scaffold_summary_reports_only_the_gitignore_change_on_a_gitignore_only_repair` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19803-19882` `init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19891-19931` `init_project_gitignores_the_store_conn_secret_file_idempotently` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19940-19954` `a_group_or_other_readable_secret_file_mode_is_flagged_owner_only_is_not` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:19966-20014` `init_project_still_writes_the_dash_ignore_lines_when_a_broader_rule_covers_them` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20023-20085` `scaffold_agents_and_workflow_reference_the_same_canonical_set` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20092-20135` `init_scaffolds_only_the_workflow_referenced_agents` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20142-20171` `get_referenced_agent_ids_reads_the_scaffolded_workflows_fleet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20178-20210` `write_if_absent_wrote_kept_and_errors_naming_the_artifact` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20218-20235` `parse_setup_args_reads_the_agents_directory_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20242-20287` `import_agents_copies_and_normalizes_the_identity_field` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20293-20330` `import_agents_refuses_to_overwrite_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20336-20349` `import_agents_validates_and_rejects_a_malformed_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20358-20380` `import_agents_rejects_an_id_colliding_with_an_existing_agent` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20386-20406` `import_agents_rejects_a_duplicate_id_within_one_import` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20412-20433` `import_agents_rejects_an_agent_with_a_blank_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20440-20462` `import_agents_runs_full_validation_and_rejects_a_broken_project` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20468-20487` `meta_object_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20496-20510` `meta_description` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20517-20525` `strip_line_comments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20539-20748` `workflow_is_a_thin_courier_driver_with_per_unit_phase_labels` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20757-20768` `the_step_schema_admits_the_attention_array` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20774-20796` `js_function_body` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20812-20867` `phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20880-20918` `meta_matches_reality_drops_integrate_and_the_unit_stage_construction` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:20937-21023` `the_driver_relays_each_attention_entry_as_a_narrator_log_line` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21030-21043` `merge_hung_attention_does_nothing_when_not_newly_hung` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21051-21063` `merge_hung_attention_defers_to_an_existing_budget_halt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21072-21108` `merge_hung_attention_lands_in_canonical_position_alongside_other_signals` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21126-21174` `workflow_step_courier_prompt_is_foreground_and_honest` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21184-21193` `step_courier_prompt` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21211-21278` `workflow_step_courier_waits_on_an_auto_backgrounded_step` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21293-21348` `workflow_driver_guards_a_null_step_before_dereferencing_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21361-21418` `workflow_meta_description_is_a_user_facing_tagline_free_of_plumbing_terms` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21425-21449` `setup_runs_npm_install_or_reports_a_clear_error` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21455-21480` `workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21482-21488` `npm_available` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21499-21542` `format_stats_prints_all_four_metrics` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21548-21564` `format_stats_handles_zeroed_metrics_without_nan` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21570-21587` `format_canary_stats_reports_findings_raised_by_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21592-21602` `format_canary_stats_reports_a_zero_findings_count_honestly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21608-21614` `format_canary_stats_omits_the_findings_volume_section_when_empty` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21619-21636` `progress_outcome` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21643-21655` `format_progress_line_names_id_verdict_and_none_when_nothing_caught` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21661-21678` `format_progress_line_reports_a_wrong_verdict_and_every_catching_tier` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21686-21717` `format_canary_stats_renders_na_for_a_tier_with_unattributed_correct_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21724-21743` `format_canary_stats_renders_the_real_zero_when_attribution_was_fully_measured` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21752-21771` `format_canary_stats_reports_control_items_and_false_positives` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21779-21794` `format_canary_stats_reports_zero_false_positives_honestly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21801-21830` `format_canary_stats_reports_the_model_pinning_header_when_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21836-21842` `format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21852-21894` `format_stats_surfaces_parallelism_retention_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21901-21941` `format_stats_surfaces_spawn_timing_and_reports_unpaired_separately` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21947-21953` `format_stats_spawn_timing_reports_no_spawns_when_none_recorded` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21960-21987` `format_stats_spawn_timing_no_spawns_line_requires_both_empty_and_zero_unpaired` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:21994-22009` `format_stats_spawn_timing_all_unpaired_is_not_reported_as_no_spawns` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22017-22044` `parallelism_retention_line_is_single_sourced_and_warns_below_the_floor` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22051-22096` `stats_discloses_when_no_verdict_was_recorded_on_this_driver` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22107-22180` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22188-22225` `stats_discloses_cause_split_remainder_when_fewer_causes_than_rejects` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22230-22236` `cmd_stats_rejects_extra_arguments` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22239-22284` `baseline_run_slice_selects_a_run_by_id_including_a_middle_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22287-22318` `format_stats_diff_flags_only_the_changed_rows` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22330-22348` `build_environment_report_with_a_wrapper_lists_wrapper_cache_dir_and_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22355-22372` `build_environment_report_with_no_wrapper_omits_cache_dir_but_keeps_budget` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22378-22388` `build_environment_report_zero_max_concurrent_reports_unlimited` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22394-22401` `build_environment_report_reports_mutation_on` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22406-22413` `build_environment_report_reports_mutation_off` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22431-22450` `order_signature_advisories_names_the_stream_count_range_and_repair_doc` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22454-22456` `order_signature_advisories_is_empty_when_no_signatures_are_given` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22464-22470` `drift_change` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22476-22505` `model_drift_advisory_is_a_soft_note_for_snapshot_only_drift` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22510-22521` `model_drift_advisory_stays_a_warning_when_any_change_is_a_real_model_repoint` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22525-22527` `model_drift_advisory_is_none_when_nothing_changed` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22532-22548` `index_staleness_message_names_every_kind_of_disagreement_and_the_fix` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22551-22574` `bloat_advisory_is_none_at_or_below_the_threshold_and_named_above_it` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22582-22597` `assert_advisory_for_never_fabricates_a_missing_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22600-22602` `bloat_advisory_for_never_fabricates_a_store_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22607-22625` `retired_entities_advisory_is_none_at_zero_and_named_with_correct_pluralization` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22628-22633` `retired_entities_advisory_for_never_fabricates_a_graph_that_does_not_exist` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22636-22670` `retired_entities_advisory_for_reads_the_projectors_own_counting_authority` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22677-22684` `scaffold_workflow_declares_build_wrapper_auto` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22692-22706` `init_project_never_clobbers_an_existing_build_section` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22709-22726` `parse_replay_args_requires_a_run_and_a_rev_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22738-22759` `cmd_stats_on_a_never_run_project_says_no_runs_and_creates_no_db` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22745-22747` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22767-22814` `a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22772-22774` `drop` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22820-22823` `no_runs_message_points_at_rigger_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22829-22836` `seed_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22842-22854` `stats_lines_absent_db_returns_none_and_creates_no_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22861-22877` `stats_lines_existing_db_with_empty_run_stream_returns_none` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22885-22914` `stats_lines_does_not_read_another_projects_namespaced_run` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22921-22952` `stats_lines_existing_run_renders_metric_lines` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:22975-23044` `stats_lines_pairs_recorded_spawn_request_and_result_into_spawn_timing` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23052-23064` `result_of_at_absent_db_reads_as_unreported_and_creates_no_file` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23070-23090` `result_of_at_unrecorded_spawn_reads_as_unreported` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23098-23122` `result_of_at_reads_a_self_reported_result_so_it_is_not_clobbered` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23128-23157` `result_of_at_is_namespace_scoped` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23164-23176` `cmd_reported_requires_exactly_one_id` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23187-23200` `pgid_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23211-23236` `detach_process_group_places_the_child_in_its_own_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23245-23266` `a_child_spawned_without_detachment_inherits_the_parent_process_group` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23280-23300` `spawn_run_dashboard_detached_session_detaches_the_dash` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23315-23335` `report_of` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23346-23369` `a_compaction_that_failed_after_the_deletes_is_reported_beside_the_counts` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23380-23403` `a_prune_that_shed_nothing_is_justified_by_this_log_not_by_when_it_was_written` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23414-23425` `a_pass_that_deleted_nothing_but_reclaimed_space_reports_the_reclamation` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23430-23461` `a_prune_that_shed_rows_explains_the_duplication_a_deduplicated_log_still_accumulates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23475-23510` `an_unmeasurable_database_is_not_reported_as_a_checkpoint_a_reader_declined` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23514-23516` `no_live_units` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23520-23538` `live_writer_reasons_is_empty_only_when_all_four_facts_are_quiet` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23544-23563` `refusal_names_a_held_step_lock_and_the_force_live_override_owning_the_risk` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23568-23576` `refusal_names_a_non_terminal_unit_between_spawn_rounds` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23580-23592` `refusal_names_every_in_flight_spawn_id_and_the_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23596-23603` `refusal_names_the_driver_registration_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23608-23620` `refusal_names_every_applicable_reason_together_not_just_the_first` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23626-23655` `refuse_derived_reset_if_live_fails_safe_on_a_malformed_spawn_event` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23661-23682` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23689-23706` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23714-23723` `watch_test_store` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23746-23779` `store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_is_found` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23793-23824` `scratch_defaults_reads_the_owning_roots_config_with_no_agents_fleet_present` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23827-23852` `watch_once_on_a_clean_store_reports_no_anomalies` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23855-23859` `parse_watch_args_defaults_to_streaming_with_the_default_interval` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23866-23888` `parse_watch_args_accepts_once_and_interval_together_in_either_order` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23891-23895` `parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:23903-24023` `watch_once_on_the_seeded_store_reports_one_line_per_anomaly` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24030-24049` `the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24057-24085` `watch_once_reports_dash_not_serving_when_the_marker_names_a_dead_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24092-24122` `watch_once_reports_no_anomaly_when_the_dash_marker_names_a_real_serving_holder` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24127-24150` `runs_menu_line_names_the_measured_counts_and_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24153-24182` `derived_menu_line_sums_the_measured_duplicate_counts_and_names_the_flag` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24189-24204` `derived_menu_line_on_a_server_backend_says_so_instead_of_a_fabricated_count` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24216-24291` `implementer_persona_pins_the_seeded_mutation_step_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).
  - `src/main.rs:24308-24345` `implementer_persona_pins_the_seeded_mutation_scratch_root_registration_contract` - defined inside a #[cfg(test)] test module; proposed home groups it with that file's own test suite pending consolidation (spec 85 section 5).

### Unassigned (165 functions)

- `src/conductor.rs:349-378` `glob_matches` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:532-548` `input_digest` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1367-1379` `replay_trajectory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:1383-1938` `run` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:2017-2109` `compute_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:8688-8695` `fnv1a_64` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:8700-8709` `verified_evidence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:9047-9094` `render_capped_section` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:9275-9278` `plain_node_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:9409-9425` `confidence_tier_radius` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:9435-9489` `files_reachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:9749-9754` `same_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:9761-9776` `sanitize_for_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:9781-9783` `integrates` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:9962-9971` `fan_out_template_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10004-10010` `implement_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10094-10099` `producer_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10198-10206` `fan_out_lenses` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10245-10267` `coverage_gap` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/conductor.rs:10312-10349` `validate_acyclic` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
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
- `src/main.rs:1099-1133` `enforce_definition_pin` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1142-1167` `normalize_origin_url` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1172-1188` `origin_url_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1194-1199` `mint_project_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1219-1233` `decide_migration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1406-1463` `main` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1635-1637` `usage` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1639-1644` `db_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1675-1698` `walk_stores_from` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1711-1732` `main_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1941-1979` `result_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:1994-1998` `load_run_config` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2073-2093` `acquire_step_lock` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2623-2636` `merge_hung_attention` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2777-2780` `reap_then_remove_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2791-2805` `reap_then_remove_worktree` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:2967-2980` `result_of_at` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3069-3093` `warn_on_run_branch_divergence` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3106-3115` `anchor_run_branch` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3139-3153` `refuse_when_base_unreachable` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3225-3341` `run_cli` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3359-3400` `fresh_run_if_requested` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3690-3707` `load_criteria` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3893-3938` `definition_body` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3952-3970` `derive_extent_end` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:3977-3982` `derive_extent_end` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4339-4368` `stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4389-4403` `parallelism_retention_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4466-4487` `append_spawn_timing` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4492-4494` `fmt_duration` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4522-4632` `append_review_quality` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4661-4677` `canary_stats_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4807-4851` `model_drift_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:4860-4877` `order_signature_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5374-5384` `started_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5405-5438` `candidate_reaches_gate` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5482-5496` `baseline_run_slice` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5499-5503` `run_started_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5510-5556` `materialize_config_at_rev` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5638-5656` `start_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5663-5678` `spawn_run_dashboard` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5765-5771` `detach_process_group` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:5774-5774` `detach_process_group` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6008-6063` `spawn_run_dashboard_detached` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6599-6649` `watch_and_self_reap_on_idle` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:6854-6856` `instance_rigger_dir` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7416-7424` `status_blocker_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7433-7439` `release_ready_lines` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7560-7773` `watch_poll` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7925-7931` `runs_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:7938-7960` `derived_menu_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8128-8217` `derived_prune_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8530-8534` `graph_node_id` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8593-8603` `peer_decision_line` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8607-8616` `json_str_array` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8620-8629` `json_type_name` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8725-8746` `build_result` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:8994-9006` `fold_recorded_result_into_graph` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9219-9248` `build_environment_report` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9256-9295` `validate_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9302-9328` `index_staleness_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9381-9390` `retired_entities_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9403-9409` `retired_entities_advisory_for` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9504-9515` `missing_gitsemver_binary_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9527-9547` `behind_the_tree_message` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9566-9578` `behind_the_tree_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9598-9612` `drift_side` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9661-9682` `uncommitted_rigger_advisory` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9690-9705` `dirty_tracked_paths` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9745-9782` `residue_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9793-9810` `leaked_process_advisories` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:9910-9956` `current_run_units` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10041-10060` `local_unit_branches` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10068-10129` `scan_residue` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10206-10225` `dir_size_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10247-10257` `build_cache_tombstone_path` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10356-10369` `human_size` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10601-10611` `dead_spawn_leaf_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10621-10628` `looks_like_run_container` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10834-10846` `owning_repo_root` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:10924-11032` `init_project` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11140-11168` `get_referenced_agent_ids` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11467-11582` `precommit_block` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:11613-11665` `compose_precommit_bytes` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12020-12111` `import_agents` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12120-12150` `normalize_identity` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12155-12161` `top_level_key` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12438-12453` `select_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.
- `src/main.rs:12461-12463` `select_reindex_grounder` - no impl-block or naming-convention rule matched this free function; flagged for manual triage in the follow-up refactor spec.

## 2. Duplication Catalog

688 clusters (3248 total sites) across `src/` and `tests/`, found by `tests/simplification_audit.rs`'s deterministic normalized-token-shingle Jaccard pass (8-token shingles, threshold 0.72) plus five mandatory mechanical sweeps. Strict definition (spec 85 Goal): any logic present in more than one place anywhere in the codebase is a violation, with no "small enough to duplicate" exemption.

### Mandatory sweeps

- **Command::new call sites**: 267 site(s) - `dup-0006`
- **/proc-path string literals**: 59 site(s) - `dup-0128`
- **sqlite Connection::open call sites**: 46 site(s) - `dup-0106`
- **.rigger-path string literals**: 655 site(s) - `dup-0052`
- **error-shaping helper functions**: 5 site(s) - `dup-0210`

### Clusters (225 exact, 387 near, 76 semantic)

#### `dup-0001` (near, 2 sites)

Proposed home: `blast_radius_eval::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blast_radius_eval.rs:216-231` `width_threshold_is_the_tier_width_nearest_rank_percentile`
- `src/blast_radius_eval.rs:234-242` `full_fraction_spans_all_light_to_collapse`

#### `dup-0002` (exact, 9 sites)

Proposed home: `a new shared module (sites span 9 files: src/blocker.rs, src/dash.rs, src/ledger.rs, src/main.rs, src/metrics.rs, src/run.rs, src/watch.rs, tests/dash_run_tree_spine.rs, tests/grep_fallback_metric_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blocker.rs:275-277` `ev`
- `src/dash.rs:5041-5043` `ev`
- `src/ledger.rs:645-647` `ev`
- `src/main.rs:13990-13992` `ev`
- `src/metrics.rs:1400-1402` `ev`
- `src/run.rs:493-495` `ev`
- `src/watch.rs:607-609` `ev`
- `tests/dash_run_tree_spine.rs:51-53` `ev`
- `tests/grep_fallback_metric_periphery.rs:50-52` `run_ev`

#### `dup-0003` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/blocker.rs, src/dash.rs, tests/dash_run_tree_spine.rs, tests/grep_fallback_metric_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blocker.rs:279-284` `positioned`
- `src/dash.rs:5047-5052` `positioned`
- `tests/dash_run_tree_spine.rs:57-62` `positioned`
- `tests/grep_fallback_metric_periphery.rs:69-74` `positioned`

#### `dup-0004` (near, 3 sites)

Proposed home: `blocker::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/blocker.rs:361-374` `reject_recurrence_line_shows_n_over_max`
- `src/blocker.rs:377-393` `reject_recurrence_line_carries_the_recorded_cause`
- `src/blocker.rs:396-416` `reject_recurrence_line_carries_the_latest_of_several_causes`

#### `dup-0005` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/budget.rs, tests/no_os_kill_test_helper_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/budget.rs:128-136` `wait_until`
- `tests/no_os_kill_test_helper_periphery.rs:64-72` `wait_until`

#### `dup-0006` (semantic, 267 sites)

Proposed home: `a single injected process-spawn port every Command::new site routes through instead of constructing its own Command`

mandatory sweep: Command::new call sites - 267 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/budget.rs:208-208` `Command::new`
- `src/budget.rs:246-246` `Command::new`
- `src/budget.rs:257-257` `Command::new`
- `src/conductor.rs:17489-17489` `Command::new`
- `src/conductor.rs:21308-21308` `Command::new`
- `src/conductor.rs:21926-21926` `Command::new`
- `src/conductor.rs:22000-22000` `Command::new`
- `src/conductor.rs:25747-25747` `Command::new`
- `src/conductor.rs:25845-25845` `Command::new`
- `src/conductor.rs:25930-25930` `Command::new`
- `src/conductor.rs:26222-26222` `Command::new`
- `src/conductor.rs:26552-26552` `Command::new`
- `src/conductor.rs:26582-26582` `Command::new`
- `src/conductor.rs:27054-27054` `Command::new`
- `src/conductor.rs:30698-30698` `Command::new`
- `src/conductor.rs:31359-31359` `Command::new`
- `src/conductor.rs:31857-31857` `Command::new`
- `src/conductor.rs:31864-31864` `Command::new`
- `src/conductor.rs:31954-31954` `Command::new`
- `src/conductor.rs:32011-32011` `Command::new`
- `src/conductor.rs:32067-32067` `Command::new`
- `src/conductor.rs:33312-33312` `Command::new`
- `src/conductor.rs:33330-33330` `Command::new`
- `src/conductor.rs:33360-33360` `Command::new`
- `src/conductor.rs:33551-33551` `Command::new`
- `src/conductor.rs:33864-33864` `Command::new`
- `src/dash.rs:4961-4961` `Command::new`
- `src/driver/cli.rs:45-45` `Command::new`
- `src/gate.rs:711-711` `Command::new`
- `src/gate.rs:720-720` `Command::new`
- `src/gate.rs:1627-1627` `Command::new`
- `src/main.rs:1173-1173` `Command::new`
- `src/main.rs:1712-1712` `Command::new`
- `src/main.rs:2794-2794` `Command::new`
- `src/main.rs:3607-3607` `Command::new`
- `src/main.rs:5522-5522` `Command::new`
- `src/main.rs:5549-5549` `Command::new`
- `src/main.rs:5666-5666` `Command::new`
- `src/main.rs:5795-5795` `Command::new`
- `src/main.rs:9465-9465` `Command::new`
- `src/main.rs:9484-9484` `Command::new`
- `src/main.rs:9662-9662` `Command::new`
- `src/main.rs:10042-10042` `Command::new`
- `src/main.rs:11105-11105` `Command::new`
- `src/main.rs:11687-11687` `Command::new`
- `src/main.rs:11821-11821` `Command::new`
- `src/main.rs:12477-12477` `Command::new`
- `src/main.rs:12966-12966` `Command::new`
- `src/main.rs:12999-12999` `Command::new`
- `src/main.rs:13043-13043` `Command::new`
- `src/main.rs:13112-13112` `Command::new`
- `src/main.rs:15209-15209` `Command::new`
- `src/main.rs:15256-15256` `Command::new`
- `src/main.rs:15366-15366` `Command::new`
- `src/main.rs:15376-15376` `Command::new`
- `src/main.rs:15393-15393` `Command::new`
- `src/main.rs:15529-15529` `Command::new`
- `src/main.rs:15541-15541` `Command::new`
- `src/main.rs:15677-15677` `Command::new`
- `src/main.rs:17019-17019` `Command::new`
- `src/main.rs:17121-17121` `Command::new`
- `src/main.rs:17187-17187` `Command::new`
- `src/main.rs:17193-17193` `Command::new`
- `src/main.rs:20737-20737` `Command::new`
- `src/main.rs:21483-21483` `Command::new`
- `src/main.rs:23214-23214` `Command::new`
- `src/main.rs:23248-23248` `Command::new`
- `src/worktree.rs:1369-1369` `Command::new`
- `src/worktree.rs:2451-2451` `Command::new`
- `src/worktree.rs:2738-2738` `Command::new`
- `src/worktree.rs:3443-3443` `Command::new`
- `src/worktree.rs:3449-3449` `Command::new`
- `tests/adaptive_labels_periphery.rs:66-66` `Command::new`
- `tests/adaptive_labels_periphery.rs:105-105` `Command::new`
- `tests/build_watch_paths.rs:43-43` `Command::new`
- `tests/build_watch_paths.rs:60-60` `Command::new`
- `tests/canary_model_drift_periphery.rs:41-41` `Command::new`
- `tests/cause_wire_periphery.rs:56-56` `Command::new`
- `tests/cause_wire_periphery.rs:76-76` `Command::new`
- `tests/change_path_revert_periphery.rs:60-60` `Command::new`
- `tests/change_path_revert_periphery.rs:104-104` `Command::new`
- `tests/change_path_revert_periphery.rs:131-131` `Command::new`
- `tests/cli.rs:24-24` `Command::new`
- `tests/cli.rs:50-50` `Command::new`
- `tests/cli.rs:113-113` `Command::new`
- `tests/cli.rs:127-127` `Command::new`
- `tests/cli.rs:190-190` `Command::new`
- `tests/cli.rs:1691-1691` `Command::new`
- `tests/cli.rs:4514-4514` `Command::new`
- `tests/cli.rs:4739-4739` `Command::new`
- `tests/cli.rs:4931-4931` `Command::new`
- `tests/cli.rs:5181-5181` `Command::new`
- `tests/cli.rs:5343-5343` `Command::new`
- `tests/cli.rs:10705-10705` `Command::new`
- `tests/cli.rs:10715-10715` `Command::new`
- `tests/cli.rs:10747-10747` `Command::new`
- `tests/cli.rs:13405-13405` `Command::new`
- `tests/cli.rs:14170-14170` `Command::new`
- `tests/cli.rs:14223-14223` `Command::new`
- `tests/cli.rs:19375-19375` `Command::new`
- `tests/cli.rs:19444-19444` `Command::new`
- `tests/cli.rs:19517-19517` `Command::new`
- `tests/cli.rs:19598-19598` `Command::new`
- `tests/cli.rs:19774-19774` `Command::new`
- `tests/cli.rs:19834-19834` `Command::new`
- `tests/cli.rs:19942-19942` `Command::new`
- `tests/cli.rs:19970-19970` `Command::new`
- `tests/cli.rs:20091-20091` `Command::new`
- `tests/cli.rs:20151-20151` `Command::new`
- `tests/cli.rs:20200-20200` `Command::new`
- `tests/cli.rs:20238-20238` `Command::new`
- `tests/cli.rs:20300-20300` `Command::new`
- `tests/cli.rs:20379-20379` `Command::new`
- `tests/cli.rs:20437-20437` `Command::new`
- `tests/cli.rs:20483-20483` `Command::new`
- `tests/code_lens_overview_collapse_viz.rs:40-40` `Command::new`
- `tests/code_lens_overview_collapse_viz.rs:102-102` `Command::new`
- `tests/common/mod.rs:118-118` `Command::new`
- `tests/community_detection_cli.rs:59-59` `Command::new`
- `tests/community_detection_cli.rs:81-81` `Command::new`
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
- `tests/files_lens_directory_hulls_viz.rs:50-50` `Command::new`
- `tests/files_lens_directory_hulls_viz.rs:175-175` `Command::new`
- `tests/files_lens_directory_hulls_viz.rs:309-309` `Command::new`
- `tests/gate_store_fence_periphery.rs:180-180` `Command::new`
- `tests/gate_store_fence_periphery.rs:497-497` `Command::new`
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
- `tests/heartbeat_write_read_agree_periphery.rs:55-55` `Command::new`
- `tests/heartbeat_write_read_agree_periphery.rs:69-69` `Command::new`
- `tests/meta_phases_declaration_periphery.rs:119-119` `Command::new`
- `tests/metadata_card_handoff_viz.rs:43-43` `Command::new`
- `tests/metadata_card_handoff_viz.rs:151-151` `Command::new`
- `tests/migration_is_deliberate_periphery.rs:450-450` `Command::new`
- `tests/migration_is_deliberate_periphery.rs:464-464` `Command::new`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:55-55` `Command::new`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:141-141` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:28-28` `Command::new`
- `tests/no_os_kill_test_helper_periphery.rs:46-46` `Command::new`
- `tests/phase_of_role_mapping_periphery.rs:95-95` `Command::new`
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
- `tests/reap_before_removal_periphery.rs:339-339` `Command::new`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:41-41` `Command::new`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:53-53` `Command::new`
- `tests/reset_build_cache_periphery.rs:38-38` `Command::new`
- `tests/reset_build_cache_periphery.rs:278-278` `Command::new`
- `tests/reset_build_cache_periphery.rs:290-290` `Command::new`
- `tests/reset_build_cache_periphery.rs:368-368` `Command::new`
- `tests/reset_build_cache_periphery.rs:386-386` `Command::new`
- `tests/reset_derived_compaction.rs:41-41` `Command::new`
- `tests/reset_derived_compaction.rs:52-52` `Command::new`
- `tests/reset_derived_compaction_periphery.rs:602-602` `Command::new`
- `tests/reset_derived_compaction_periphery.rs:617-617` `Command::new`
- `tests/reset_derived_compaction_periphery.rs:2599-2599` `Command::new`
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
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:47-47` `Command::new`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:66-66` `Command::new`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:139-139` `Command::new`
- `tests/spawn_target_dir_periphery.rs:64-64` `Command::new`
- `tests/spec_lint.rs:25-25` `Command::new`
- `tests/step_attention_periphery.rs:151-151` `Command::new`
- `tests/step_attention_periphery.rs:466-466` `Command::new`
- `tests/step_attention_periphery.rs:475-475` `Command::new`
- `tests/step_attention_periphery.rs:701-701` `Command::new`
- `tests/store_content_identity_periphery.rs:1341-1341` `Command::new`
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
- `tests/unified_traversal_grounding.rs:582-582` `Command::new`
- `tests/validate_advisories.rs:49-49` `Command::new`
- `tests/validate_advisories.rs:58-58` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:68-68` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:95-95` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:114-114` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:140-140` `Command::new`
- `tests/validate_behind_the_tree_periphery.rs:202-202` `Command::new`
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

#### `dup-0008` (exact, 11 sites)

Proposed home: `a new shared module (sites span 10 files: src/canary.rs, src/conductor.rs, src/config.rs, tests/canary_false_positives_periphery.rs, tests/canary_findings_volume_periphery.rs, tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_lens_fanout_periphery.rs, tests/canary_progress_hook_periphery.rs, tests/canary_tolerant_attribution_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/canary.rs:928-933` `agent`
- `src/canary.rs:1037-1042` `with_anchor`
- `src/conductor.rs:10810-10815` `agent`
- `src/config.rs:2725-2730` `agent_def`
- `tests/canary_false_positives_periphery.rs:123-128` `agent`
- `tests/canary_findings_volume_periphery.rs:109-114` `agent`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:41-46` `agent`
- `tests/canary_lens_fanout_periphery.rs:107-112` `agent`
- `tests/canary_progress_hook_periphery.rs:47-52` `agent`
- `tests/canary_tolerant_attribution_periphery.rs:88-93` `agent`
- `tests/canary_unattributed_rejects_periphery.rs:110-115` `agent`

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
- `tests/simplification_audit.rs:1780-1782` `is_keyword`

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

- `src/conductor.rs:291-293` `gate_verdict_key`
- `src/conductor.rs:303-305` `gate_skip_key`
- `src/conductor.rs:316-318` `postmerge_gate_verdict_key`

#### `dup-0025` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:325-327` `compensation_queued_key`
- `src/spawn.rs:90-92` `spawn_id`

#### `dup-0026` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:505-507` `unit_of_gate_key`
- `src/spawn.rs:177-179` `unit_of`

#### `dup-0027` (near, 14 sites)

Proposed home: `a new shared module (sites span 9 files: src/conductor.rs, src/eventstore/namespace.rs, src/eventstore/sqlite.rs, src/grounder/mod.rs, src/main.rs, src/spawn.rs, src/worktree.rs, tests/canary_model_drift_periphery.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:621-623` `deferred_gate_verdict_key`
- `src/conductor.rs:632-634` `deferred_gate_failed_key`
- `src/conductor.rs:8892-8894` `build_system_prompt`
- `src/conductor.rs:8919-8926` `review_protocol`
- `src/eventstore/namespace.rs:69-71` `prefix_for`
- `src/eventstore/sqlite.rs:1404-1406` `successor`
- `src/grounder/mod.rs:207-213` `retired_grounder_error`
- `src/main.rs:11314-11316` `skill_source_rel`
- `src/main.rs:12331-12337` `spec_lint_next_step`
- `src/spawn.rs:65-67` `lens_role`
- `src/spawn.rs:143-145` `speculation_group_id`
- `src/worktree.rs:790-792` `shared_build_cache_guard_path`
- `tests/canary_model_drift_periphery.rs:140-142` `prose_claiming`
- `tests/reset_derived_compaction_periphery.rs:2816-2818` `derived_key_for`

#### `dup-0028` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:655-657` `from`
- `src/conductor.rs:660-662` `from`

#### `dup-0029` (semantic, 2 sites)

Proposed home: `conductor::review_outcome - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:787-794` `approved`
- `src/conductor.rs:795-802` `rejected`

#### `dup-0030` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:1121-1123` `is_parked`
- `src/conductor.rs:1159-1161` `is_budget_refused`
- `src/conductor.rs:1237-1239` `is_degenerate_reviewer`
- `src/conductor.rs:1280-1282` `is_verdict_channel_mismatch`

#### `dup-0031` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:1150-1154` `budget_refused`
- `src/conductor.rs:1265-1275` `verdict_channel_mismatch`

#### `dup-0032` (semantic, 2 sites)

Proposed home: `one shared `append_and_fold_batch` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:2466-2488` `append_and_fold_batch`
- `src/ingest.rs:47-87` `append_and_fold_batch`

#### `dup-0033` (exact, 2 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:2672-2674` `recorded_gate_verdict`
- `src/conductor.rs:2684-2686` `cached_green_verdict`

#### `dup-0034` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3079-3081` `spawn_is_recorded`
- `src/dash.rs:2032-2034` `is_shared`

#### `dup-0035` (exact, 4 sites)

Proposed home: `conductor::run_ctx`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:3123-3125` `budget_broke`
- `src/conductor.rs:3131-3133` `parked`
- `src/conductor.rs:3139-3141` `manual_review_pending`
- `src/conductor.rs:3146-3148` `budget_halted`

#### `dup-0036` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/distiller.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:8640-8642` `normalize_ws`
- `src/distiller.rs:60-62` `normalize`
- `tests/cli.rs:24762-24764` `normalize_ws`

#### `dup-0037` (semantic, 2 sites)

Proposed home: `one shared `normalize_ws` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:8640-8642` `normalize_ws`
- `tests/cli.rs:24762-24764` `normalize_ws`

#### `dup-0038` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:9700-9702` `unit_branch`
- `src/eventstore/sqlite.rs:1323-1325` `key_expr`

#### `dup-0039` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:9728-9733` `review_worktree_dir`
- `src/conductor.rs:9740-9742` `review_branch`

#### `dup-0040` (near, 4 sites)

Proposed home: `conductor::stub`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10620-10627` `prompts_for`
- `src/conductor.rs:10630-10637` `dirs_for`
- `src/conductor.rs:10641-10647` `system_prompt_for`
- `src/conductor.rs:10651-10653` `title_for`

#### `dup-0041` (near, 12 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/eventstore/mod.rs, tests/build_env_authority_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10663-10665` `spawn_ids`
- `src/conductor.rs:29191-29193` `calls`
- `src/conductor.rs:29194-29196` `targets`
- `src/conductor.rs:29200-29202` `store_fences`
- `src/conductor.rs:29203-29205` `build_cache_guards`
- `src/conductor.rs:29206-29208` `build_cache_dirs`
- `src/conductor.rs:29359-29361` `calls`
- `src/eventstore/mod.rs:202-204` `last`
- `src/eventstore/mod.rs:515-517` `recv`
- `src/eventstore/mod.rs:525-527` `try_recv`
- `src/eventstore/mod.rs:530-532` `err`
- `tests/build_env_authority_periphery.rs:378-380` `outputs`

#### `dup-0042` (exact, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10669-10676` `spawn_count`
- `src/conductor.rs:34414-34421` `count`
- `src/conductor.rs:35169-35176` `count`

#### `dup-0043` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/eventstore/mod.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10680-10686` `spawned`
- `src/eventstore/mod.rs:375-377` `covers`

#### `dup-0044` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/config.rs, src/driver/replay.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10820-10826` `agent_with_prompt`
- `src/config.rs:1641-1647` `agent`
- `src/driver/replay.rs:1257-1266` `stage`

#### `dup-0045` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:10877-10901` `coverage_gate_refuses_an_uncovered_criterion`
- `src/conductor.rs:24369-24401` `coverage_gap_flags_a_spec_defect_and_errors`
- `src/conductor.rs:24448-24482` `planner_leaving_a_gap_flags_a_spec_defect`
- `src/conductor.rs:24485-24519` `gate_only_stage_is_a_coverage_proxy_gap`

#### `dup-0046` (near, 8 sites)

Proposed home: `a new shared module (sites span 3 files: src/conductor.rs, src/driver/replay.rs, tests/replan_episode_identity.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:11233-11260` `supersede_cfg`
- `src/conductor.rs:18145-18167` `sha_stamp_cfg`
- `src/conductor.rs:18454-18474` `degenerate_reviewer_cfg`
- `src/conductor.rs:29261-29283` `content_cache_cfg`
- `src/conductor.rs:34318-34357` `critique_cfg`
- `src/driver/replay.rs:1726-1746` `reviewed_unit_cfg`
- `tests/replan_episode_identity.rs:234-296` `two_episode_cfg`
- `tests/replan_episode_identity.rs:1050-1087` `resume_seam_cfg`

#### `dup-0047` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:12029-12125` `a_later_episodes_proposal_supersedes_an_earlier_episodes_planner_unit`
- `src/conductor.rs:12188-12267` `a_planner_proposal_with_no_data_episode_field_still_supersedes_via_meta_spawn`

#### `dup-0048` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:12270-12397` `a_same_id_refine_restamps_its_episode_so_its_own_episodes_sibling_does_not_reap_it`
- `src/conductor.rs:12400-12522` `a_same_id_refine_survives_its_own_episodes_sibling_add_walked_first`
- `src/conductor.rs:12857-12951` `a_legacy_proposal_never_supersedes_an_identified_episodes_owner_even_when_logged_later`

#### `dup-0049` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:12559-12577` `append_one`
- `src/conductor.rs:12704-12721` `append_legacy`
- `src/conductor.rs:12723-12741` `append_identified`

#### `dup-0050` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:12579-12591` `shape`
- `src/conductor.rs:12743-12755` `shape`

#### `dup-0051` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:13071-13127` `a_verbatim_copy_still_supersedes_its_baseline`
- `src/conductor.rs:13130-13204` `a_paraphrased_proposal_matches_its_baseline_by_id_not_prose`
- `src/conductor.rs:13207-13291` `a_bracketed_id_echo_with_a_paraphrase_still_supersedes_exactly_once`
- `src/conductor.rs:13294-13366` `a_stale_non_matching_id_falls_back_to_the_verbatim_prose_match`

#### `dup-0052` (semantic, 655 sites)

Proposed home: `one .rigger-relative path-composition helper`

mandatory sweep: .rigger-path string literals - 655 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/conductor.rs:13573-13573` `"the repo's own .rigger config must load"`
- `src/config.rs:783-783` `".rigger"`
- `src/config.rs:2234-2234` `".rigger/agents/sdet-author.md"`
- `src/config.rs:2235-2235` `"the shipped .rigger/agents/sdet-author.md must exist"`
- `src/config.rs:2262-2262` `".rigger/agents/sdet.md"`
- `src/config.rs:2263-2263` `"the shipped .rigger/agents/sdet.md must exist"`
- `src/config.rs:2759-2759` `".rigger"`
- `src/config.rs:2760-2760` `"create .rigger dir"`
- `src/config.rs:2792-2792` `".rigger"`
- `src/config.rs:2793-2793` `"create .rigger dir"`
- `src/config.rs:2814-2814` `".rigger"`
- `src/config.rs:2815-2815` `"create .rigger dir"`
- `src/dash.rs:5073-5073` `"{root}/.rigger/events.db"`
- `src/dash.rs:5122-5122` `"/.rigger/events.db"`
- `src/docs.rs:203-203` `"The EVENT LOG accumulates separately from the graph, and has its own prune: `rigger \
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
- `src/docs.rs:613-613` `"description: Store hygiene for rigger's own state - growing .rigger/ disk usage, \
         the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. \
         Read this before running `rigger reset` or touching any store file by hand.\n"`
- `src/docs.rs:621-621` `"rigger keeps three stores under `.rigger/`, and only one of them holds anything \
         durable:\n"`
- `src/docs.rs:706-706` `"`rigger graph build` folds the project's source straight into `.rigger/graph.db` - \
         no run, no `RunStarted`, nothing but the code-ingest events the fold already emits. \
         It CREATES the store when the checkout is cold (`.rigger/` does not exist yet) and \
         REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and \
         it reuses the exact same walk-and-content-key ingest authority a live run uses, so a \
         standalone build and a run can never fold the same file under two different keys.\n"`
- `src/docs.rs:722-722` `"Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and \
         re-running `rigger graph build` on the empty result. Deleting the log throws away \
         truth that no rebuild can get back, and deleting only the graph is unnecessary work \
         `rigger graph build` already does FOR you, incrementally, without erasing anything \
         first. If lookups are empty, just run `rigger graph build`; only reach for \
         rigger-reset-store if you specifically mean to prune, not rebuild.\n"`
- `src/docs.rs:757-757` `"`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to \
         the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix \
         for an index that has drifted from files you just changed (a unit's own commit, a \
         rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT \
         store from the structural context graph, so it costs only the named files, never a \
         walk of the whole tree.\n"`
- `src/gate.rs:469-469` `".rigger-cache-probe-{}"`
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
- `src/main.rs:1292-1292` `"migrated project history to the durable identity: renamed {n} stream(s) \
                     from the legacy namespace {legacy:?} to the minted identity {minted:?} \
                     (.rigger/{PROJECT_ID_FILE})"`
- `src/main.rs:1359-1359` `"rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE})"`
- `src/main.rs:1470-1470` `"rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
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
- `src/main.rs:3667-3667` `"workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`."`
- `src/main.rs:6559-6559` `".rigger"`
- `src/main.rs:8250-8250` `"a `rigger step` is running right now (it holds .rigger/step.lock)"`
- `src/main.rs:9431-9431` `".rigger-workflow-provenance"`
- `src/main.rs:9675-9675` `"warning: tracked .rigger/ files have uncommitted modifications:"`
- `src/main.rs:11014-11014` `".rigger/shim/"`
- `src/main.rs:11015-11015` `".rigger/dash.url"`
- `src/main.rs:11016-11016` `".rigger/dash.marker"`
- `src/main.rs:11017-11017` `".rigger/dash.attempt"`
- `src/main.rs:11018-11018` `".rigger/store.conn"`
- `src/main.rs:11181-11181` `"minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"`
- `src/main.rs:11186-11186` `"scaffolded .rigger/workflow.yml"`
- `src/main.rs:11190-11190` `"scaffolded .rigger/agents/{{{}}}"`
- `src/main.rs:11470-11470` `r#"__BEGIN__
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
- `src/main.rs:11879-11879` `"imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)"`
- `src/main.rs:11923-11923` `"provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"`
- `src/main.rs:12071-12071` `"kept existing .rigger/agents/{name} (import never overwrites)"`
- `src/main.rs:12099-12099` `"imported .rigger/agents/{name} (id: {id})"`
- `src/main.rs:13737-13737` `"/.rigger/tmp/cargo-target/debug/rigger"`
- `src/main.rs:13738-13738` `"/.rigger/tmp/cargo-target/release/rigger"`
- `src/main.rs:15048-15048` `" M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n"`
- `src/main.rs:15058-15058` `".rigger/workflow.yml"`
- `src/main.rs:15059-15059` `".rigger/agents/sdet.md"`
- `src/main.rs:15060-15060` `".rigger/agents/new.md"`
- `src/main.rs:15061-15061` `".rigger/agents/gone.md"`
- `src/main.rs:16027-16027` `".rigger"`
- `src/main.rs:16031-16031` `".rigger"`
- `src/main.rs:16057-16057` `"probe/.rigger/events.db"`
- `src/main.rs:16058-16058` `"rigger-wt-x/.rigger/events.db"`
- `src/main.rs:16210-16210` `".rigger"`
- `src/main.rs:16245-16245` `"rigger-wt-unit-99-ghost-12345678/.rigger/events.db"`
- `src/main.rs:16280-16280` `"probe/.rigger/events.db"`
- `src/main.rs:16291-16291` `"shadow store: probe/.rigger/events.db (6B)"`
- `src/main.rs:16376-16376` `".rigger"`
- `src/main.rs:16443-16443` `".rigger"`
- `src/main.rs:16511-16511` `".rigger"`
- `src/main.rs:16590-16590` `".rigger"`
- `src/main.rs:16696-16696` `".rigger"`
- `src/main.rs:17143-17143` `".rigger"`
- `src/main.rs:17183-17183` `".rigger"`
- `src/main.rs:17365-17365` `".rigger"`
- `src/main.rs:17376-17376` `"must walk past the storeless worktree `.rigger/` to the repo's real store"`
- `src/main.rs:18303-18303` `".rigger"`
- `src/main.rs:18305-18305` `".rigger"`
- `src/main.rs:18360-18360` `".rigger"`
- `src/main.rs:18396-18396` `".rigger"`
- `src/main.rs:18438-18438` `".rigger"`
- `src/main.rs:18544-18544` `".rigger/store.conn beats the committed config"`
- `src/main.rs:19237-19237` `"{name} must be written into .rigger/shim/"`
- `src/main.rs:19786-19786` `".rigger/agents/"`
- `src/main.rs:19812-19812` `".rigger/dash.url"`
- `src/main.rs:19815-19815` `".rigger/dash.marker"`
- `src/main.rs:19818-19818` `".rigger/dash.attempt"`
- `src/main.rs:19827-19827` `".rigger/dash.url"`
- `src/main.rs:19831-19831` `".rigger/dash.marker"`
- `src/main.rs:19835-19835` `".rigger/dash.attempt"`
- `src/main.rs:19846-19846` `".rigger/dash.url"`
- `src/main.rs:19849-19849` `".rigger/dash.marker"`
- `src/main.rs:19852-19852` `".rigger/dash.attempt"`
- `src/main.rs:19861-19861` `".rigger/dash.url"`
- `src/main.rs:19864-19864` `"exactly one .rigger/dash.url ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:19869-19869` `".rigger/dash.marker"`
- `src/main.rs:19872-19872` `"exactly one .rigger/dash.marker ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:19877-19877` `".rigger/dash.attempt"`
- `src/main.rs:19880-19880` `"exactly one .rigger/dash.attempt ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:19900-19900` `".rigger/store.conn"`
- `src/main.rs:19908-19908` `".rigger/store.conn"`
- `src/main.rs:19917-19917` `".rigger/store.conn"`
- `src/main.rs:19926-19926` `".rigger/store.conn"`
- `src/main.rs:19929-19929` `"exactly one .rigger/store.conn ignore line - no duplicate accrued, got:\n{after}"`
- `src/main.rs:19970-19970` `".rigger/\n"`
- `src/main.rs:19976-19976` `".rigger/dash.url"`
- `src/main.rs:19979-19979` `".rigger/dash.marker"`
- `src/main.rs:19982-19982` `".rigger/dash.attempt"`
- `src/main.rs:19983-19983` `"setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}"`
- `src/main.rs:19991-19991` `".rigger/dash.url"`
- `src/main.rs:19992-19992` `".rigger/dash.marker"`
- `src/main.rs:19993-19993` `".rigger/dash.attempt"`
- `src/main.rs:19994-19994` `"all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"`
- `src/main.rs:20004-20004` `".rigger/dash.url"`
- `src/main.rs:20007-20007` `".rigger/dash.marker"`
- `src/main.rs:20010-20010` `".rigger/dash.attempt"`
- `src/main.rs:20270-20270` `".rigger/agents/researcher.md"`
- `src/main.rs:20299-20299` `".rigger/agents/planner.md"`
- `src/main.rs:20329-20329` `".rigger/agents/newcomer.md"`
- `src/main.rs:20377-20377` `".rigger/agents/my-planner.md"`
- `src/main.rs:20402-20402` `".rigger/agents/a-dup.md"`
- `src/main.rs:20403-20403` `".rigger/agents/b-dup.md"`
- `src/main.rs:20430-20430` `".rigger/agents/blank.md"`
- `src/main.rs:20445-20445` `".rigger/workflow.yml"`
- `src/main.rs:21474-21474` `"locate_shim must return the provisioned .rigger/shim/shim.mjs"`
- `src/main.rs:22587-22587` `".rigger"`
- `src/main.rs:22641-22641` `".rigger"`
- `src/reap.rs:423-423` `".rigger"`
- `src/reap.rs:695-695` `"a relocated/cache-home-style authorized_root with no .rigger/tmp relationship \
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
- `src/worktree.rs:733-733` `"{}/.rigger/tmp"`
- `src/worktree.rs:1851-1851` `"{repo_path}/.rigger/tmp"`
- `src/worktree.rs:1868-1868` `"~/.rigger-scratch-test"`
- `src/worktree.rs:1869-1869` `"{home}/.rigger-scratch-test"`
- `src/worktree.rs:3381-3381` `"{base}..rigger-run"`
- `src/worktree.rs:3430-3430` `".rigger"`
- `tests/architecture_current_surface.rs:107-107` `".rigger/store.conn"`
- `tests/architecture_current_surface.rs:163-163` `"docs/architecture.md must describe the system that exists today (spec 56, \
         criterion 1): it must name the store-resolution and configuration surface (the \
         committed `store:` selection, the `KURRENTDB_CONN` environment variable, and the \
         per-machine `.rigger/store.conn` secret file) and the graph inspector's real query \
         surface (the three `lens=` names and the directed `view=calls` `dir=` views). \
         Surfaces the document fails to name: {missing:#?}"`
- `tests/build_budget_slots_periphery.rs:196-196` `".rigger"`
- `tests/build_budget_slots_periphery.rs:197-197` `"create .rigger/agents"`
- `tests/build_env_authority_periphery.rs:166-166` `".rigger"`
- `tests/build_env_authority_periphery.rs:167-167` `"create .rigger/agents"`
- `tests/canary_model_drift_periphery.rs:90-90` `".rigger"`
- `tests/cause_wire_periphery.rs:66-66` `".rigger"`
- `tests/cause_wire_periphery.rs:86-86` `".rigger"`
- `tests/cause_wire_periphery.rs:104-104` `".rigger"`
- `tests/change_path_revert_periphery.rs:110-110` `".rigger"`
- `tests/change_path_revert_periphery.rs:144-144` `".rigger"`
- `tests/change_path_revert_periphery.rs:165-165` `".rigger"`
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
- `tests/cli.rs:1415-1415` `".rigger"`
- `tests/cli.rs:1447-1447` `".rigger"`
- `tests/cli.rs:1537-1537` `".rigger"`
- `tests/cli.rs:1547-1547` `".rigger"`
- `tests/cli.rs:1560-1560` `".rigger"`
- `tests/cli.rs:1616-1616` `".rigger"`
- `tests/cli.rs:1617-1617` `".rigger"`
- `tests/cli.rs:1629-1629` `".rigger"`
- `tests/cli.rs:1651-1651` `".rigger"`
- `tests/cli.rs:1689-1689` `".rigger"`
- `tests/cli.rs:1711-1711` `".rigger"`
- `tests/cli.rs:1747-1747` `".rigger"`
- `tests/cli.rs:1762-1762` `".rigger"`
- `tests/cli.rs:1920-1920` `".rigger"`
- `tests/cli.rs:2176-2176` `".rigger"`
- `tests/cli.rs:2418-2418` `".rigger"`
- `tests/cli.rs:2516-2516` `".rigger"`
- `tests/cli.rs:2557-2557` `".rigger"`
- `tests/cli.rs:2589-2589` `".rigger"`
- `tests/cli.rs:2590-2590` `"graph build must create .rigger/graph.db from a cold checkout"`
- `tests/cli.rs:2640-2640` `".rigger"`
- `tests/cli.rs:2641-2641` `"graph build must create .rigger/graph.db even when there is nothing to ingest"`
- `tests/cli.rs:2664-2664` `".rigger"`
- `tests/cli.rs:2742-2742` `".rigger"`
- `tests/cli.rs:2757-2757` `".rigger"`
- `tests/cli.rs:2805-2805` `".rigger"`
- `tests/cli.rs:2830-2830` `".rigger"`
- `tests/cli.rs:2949-2949` `".rigger"`
- `tests/cli.rs:2953-2953` `"grounding via symbols must persist the structural index to .rigger/symbols/"`
- `tests/cli.rs:3126-3126` `".rigger"`
- `tests/cli.rs:3520-3520` `".rigger/tmp/agent-scratch/"`
- `tests/cli.rs:3526-3526` `".rigger/tmp/cargo-target"`
- `tests/cli.rs:3534-3534` `"CARGO_TARGET_DIR=${REPO}/.rigger/tmp/cargo-target rigger step"`
- `tests/cli.rs:3618-3618` `".rigger"`
- `tests/cli.rs:3647-3647` `".rigger"`
- `tests/cli.rs:3757-3757` `".rigger"`
- `tests/cli.rs:3821-3821` `".rigger"`
- `tests/cli.rs:3876-3876` `".rigger"`
- `tests/cli.rs:3953-3953` `".rigger"`
- `tests/cli.rs:3982-3982` `".rigger"`
- `tests/cli.rs:4072-4072` `".rigger"`
- `tests/cli.rs:4174-4174` `".rigger"`
- `tests/cli.rs:4205-4205` `".rigger"`
- `tests/cli.rs:4259-4259` `".rigger"`
- `tests/cli.rs:4346-4346` `".rigger"`
- `tests/cli.rs:4437-4437` `".rigger"`
- `tests/cli.rs:4490-4490` `".rigger"`
- `tests/cli.rs:4606-4606` `".rigger"`
- `tests/cli.rs:4676-4676` `".rigger"`
- `tests/cli.rs:4726-4726` `".rigger"`
- `tests/cli.rs:4814-4814` `".rigger"`
- `tests/cli.rs:4871-4871` `".rigger"`
- `tests/cli.rs:4910-4910` `".rigger"`
- `tests/cli.rs:5077-5077` `".rigger"`
- `tests/cli.rs:5131-5131` `".rigger"`
- `tests/cli.rs:5170-5170` `".rigger"`
- `tests/cli.rs:5301-5301` `".rigger"`
- `tests/cli.rs:5332-5332` `".rigger"`
- `tests/cli.rs:5407-5407` `".rigger"`
- `tests/cli.rs:5486-5486` `".rigger"`
- `tests/cli.rs:5619-5619` `".rigger"`
- `tests/cli.rs:5676-5676` `".rigger"`
- `tests/cli.rs:5846-5846` `".rigger"`
- `tests/cli.rs:5861-5861` `".rigger"`
- `tests/cli.rs:5934-5934` `".rigger"`
- `tests/cli.rs:6040-6040` `".rigger"`
- `tests/cli.rs:6109-6109` `".rigger"`
- `tests/cli.rs:6156-6156` `".rigger"`
- `tests/cli.rs:6213-6213` `".rigger"`
- `tests/cli.rs:6279-6279` `".rigger"`
- `tests/cli.rs:6292-6292` `".rigger"`
- `tests/cli.rs:6415-6415` `".rigger"`
- `tests/cli.rs:6483-6483` `".rigger"`
- `tests/cli.rs:6501-6501` `".rigger"`
- `tests/cli.rs:6614-6614` `".rigger"`
- `tests/cli.rs:6714-6714` `".rigger/events.db"`
- `tests/cli.rs:6913-6913` `".rigger"`
- `tests/cli.rs:6947-6947` `".rigger"`
- `tests/cli.rs:7175-7175` `".rigger"`
- `tests/cli.rs:7265-7265` `".rigger"`
- `tests/cli.rs:7321-7321` `"/.rigger/tmp/agent-live/"`
- `tests/cli.rs:7606-7606` `".rigger"`
- `tests/cli.rs:7906-7906` `"/.rigger/tmp/agent-live/"`
- `tests/cli.rs:7907-7907` `"under RIGGER_TMPDIR the marker is not under the repo's .rigger/tmp; got: {marker_str:?}"`
- `tests/cli.rs:8688-8688` `".rigger"`
- `tests/cli.rs:8851-8851` `".rigger"`
- `tests/cli.rs:8955-8955` `".rigger"`
- `tests/cli.rs:9004-9004` `".rigger"`
- `tests/cli.rs:9106-9106` `".rigger"`
- `tests/cli.rs:9165-9165` `".rigger"`
- `tests/cli.rs:9269-9269` `".rigger"`
- `tests/cli.rs:9336-9336` `".rigger"`
- `tests/cli.rs:9478-9478` `".rigger"`
- `tests/cli.rs:9526-9526` `".rigger"`
- `tests/cli.rs:9644-9644` `".rigger"`
- `tests/cli.rs:9915-9915` `".rigger"`
- `tests/cli.rs:10014-10014` `".rigger"`
- `tests/cli.rs:10103-10103` `".rigger"`
- `tests/cli.rs:10143-10143` `".rigger"`
- `tests/cli.rs:10800-10800` `".rigger"`
- `tests/cli.rs:10962-10962` `".rigger"`
- `tests/cli.rs:11341-11341` `".rigger/workflow.yml"`
- `tests/cli.rs:11341-11341` `".rigger/agents"`
- `tests/cli.rs:11406-11406` `".rigger"`
- `tests/cli.rs:11440-11440` `".rigger/workflow.yml"`
- `tests/cli.rs:11440-11440` `".rigger/agents"`
- `tests/cli.rs:11443-11443` `".rigger/workflow.yml"`
- `tests/cli.rs:11475-11475` `".rigger"`
- `tests/cli.rs:11509-11509` `".rigger/workflow.yml"`
- `tests/cli.rs:11509-11509` `".rigger/agents"`
- `tests/cli.rs:11512-11512` `".rigger/workflow.yml"`
- `tests/cli.rs:11547-11547` `".rigger"`
- `tests/cli.rs:11586-11586` `".rigger/workflow.yml"`
- `tests/cli.rs:11586-11586` `".rigger/agents"`
- `tests/cli.rs:11589-11589` `".rigger/workflow.yml"`
- `tests/cli.rs:11623-11623` `".rigger"`
- `tests/cli.rs:11661-11661` `".rigger/workflow.yml"`
- `tests/cli.rs:11661-11661` `".rigger/agents"`
- `tests/cli.rs:11664-11664` `".rigger/workflow.yml"`
- `tests/cli.rs:11719-11719` `".rigger/workflow.yml"`
- `tests/cli.rs:11719-11719` `".rigger/agents"`
- `tests/cli.rs:11727-11727` `".rigger"`
- `tests/cli.rs:11766-11766` `".rigger/workflow.yml"`
- `tests/cli.rs:11766-11766` `".rigger/agents"`
- `tests/cli.rs:11779-11779` `".rigger"`
- `tests/cli.rs:11788-11788` `".rigger"`
- `tests/cli.rs:11790-11790` `".rigger"`
- `tests/cli.rs:11887-11887` `".rigger/workflow.yml"`
- `tests/cli.rs:11888-11888` `"validate must NOT flag a clean tracked `.rigger/` tree; stderr:\n{err}"`
- `tests/cli.rs:11897-11897` `".rigger"`
- `tests/cli.rs:11904-11904` `"validate must still succeed (exit 0) when it only FLAGS uncommitted `.rigger/` \
         changes; stderr:\n{err}"`
- `tests/cli.rs:11908-11908` `".rigger/workflow.yml"`
- `tests/cli.rs:11909-11909` `"validate must flag the tracked-but-modified `.rigger/workflow.yml` on stderr; \
         stderr:\n{err}"`
- `tests/cli.rs:11925-11925` `".rigger"`
- `tests/cli.rs:12158-12158` `".rigger"`
- `tests/cli.rs:12565-12565` `".rigger"`
- `tests/cli.rs:12709-12709` `".rigger"`
- `tests/cli.rs:12711-12711` `".rigger"`
- `tests/cli.rs:12714-12714` `".rigger"`
- `tests/cli.rs:12716-12716` `".rigger"`
- `tests/cli.rs:12744-12744` `"probe/.rigger/events.db"`
- `tests/cli.rs:12778-12778` `".rigger"`
- `tests/cli.rs:13698-13698` `".rigger"`
- `tests/cli.rs:13795-13795` `"scaffolded .rigger/workflow.yml"`
- `tests/cli.rs:13799-13799` `"scaffolded .rigger/agents/"`
- `tests/cli.rs:13831-13831` `".rigger"`
- `tests/cli.rs:13863-13863` `".rigger/agents/researcher.md"`
- `tests/cli.rs:13864-13864` `"the agent was actually imported into .rigger/agents/"`
- `tests/cli.rs:13911-13911` `".rigger"`
- `tests/cli.rs:14140-14140` `".rigger/dash.url"`
- `tests/cli.rs:14144-14144` `".rigger/dash.marker"`
- `tests/cli.rs:14148-14148` `".rigger/dash.attempt"`
- `tests/cli.rs:14157-14157` `".rigger"`
- `tests/cli.rs:14159-14159` `".rigger"`
- `tests/cli.rs:14163-14163` `".rigger"`
- `tests/cli.rs:14164-14164` `".rigger"`
- `tests/cli.rs:14166-14166` `".rigger/dash.url"`
- `tests/cli.rs:14167-14167` `".rigger/dash.marker"`
- `tests/cli.rs:14168-14168` `".rigger/dash.attempt"`
- `tests/cli.rs:14208-14208` `".claude/\n.rigger/\n"`
- `tests/cli.rs:14224-14224` `".rigger/dash.url"`
- `tests/cli.rs:14232-14232` `"the test's global config must actually ignore .rigger/dash.url (else the regression \
         guard is inconclusive)"`
- `tests/cli.rs:14255-14255` `".rigger/shim"`
- `tests/cli.rs:14256-14256` `".rigger/dash.url"`
- `tests/cli.rs:14257-14257` `".rigger/dash.marker"`
- `tests/cli.rs:14258-14258` `".rigger/dash.attempt"`
- `tests/cli.rs:14462-14462` `".rigger/project.id"`
- `tests/cli.rs:14465-14465` `".rigger/project.id"`
- `tests/cli.rs:14478-14478` `".rigger/project.id"`
- `tests/cli.rs:14567-14567` `".rigger/project.id"`
- `tests/cli.rs:14616-14616` `".rigger/project.id"`
- `tests/cli.rs:14621-14621` `".rigger/project.id"`
- `tests/cli.rs:14635-14635` `".rigger"`
- `tests/cli.rs:14778-14778` `".rigger"`
- `tests/cli.rs:14897-14897` `".rigger"`
- `tests/cli.rs:14989-14989` `".rigger"`
- `tests/cli.rs:15054-15054` `".rigger"`
- `tests/cli.rs:15124-15124` `".rigger"`
- `tests/cli.rs:15488-15488` `".rigger"`
- `tests/cli.rs:15556-15556` `".rigger"`
- `tests/cli.rs:15693-15693` `".rigger"`
- `tests/cli.rs:15892-15892` `".rigger"`
- `tests/cli.rs:16082-16082` `".rigger"`
- `tests/cli.rs:16264-16264` `".rigger"`
- `tests/cli.rs:16309-16309` `".rigger"`
- `tests/cli.rs:16721-16721` `".rigger"`
- `tests/cli.rs:16736-16736` `"the driver never recorded a dash URL in .rigger/dash.url; stderr:\n{err}"`
- `tests/cli.rs:16781-16781` `".rigger"`
- `tests/cli.rs:16844-16844` `".rigger"`
- `tests/cli.rs:16919-16919` `".rigger"`
- `tests/cli.rs:16995-16995` `".rigger"`
- `tests/cli.rs:17079-17079` `".rigger"`
- `tests/cli.rs:17186-17186` `".rigger"`
- `tests/cli.rs:17902-17902` `".rigger"`
- `tests/cli.rs:17904-17904` `".rigger"`
- `tests/cli.rs:18601-18601` `".rigger/dash.marker"`
- `tests/cli.rs:18642-18642` `".rigger/dash.marker"`
- `tests/cli.rs:18645-18645` `".rigger/dash.url"`
- `tests/cli.rs:18697-18697` `".rigger/dash.url"`
- `tests/cli.rs:18702-18702` `".rigger/dash.marker"`
- `tests/cli.rs:18758-18758` `".rigger/dash.url"`
- `tests/cli.rs:18760-18760` `".rigger/dash.marker"`
- `tests/cli.rs:18829-18829` `".rigger/dash.marker"`
- `tests/cli.rs:18887-18887` `".rigger/dash.marker"`
- `tests/cli.rs:18992-18992` `".rigger/dash.marker"`
- `tests/cli.rs:19046-19046` `".rigger/dash.marker"`
- `tests/cli.rs:19068-19068` `".rigger/dash.attempt"`
- `tests/cli.rs:19078-19078` `"a marker that LOOKS like it predates this run's own RunStarted must still be reported \
         when .rigger/dash.attempt explicitly names this exact run - proving watch_poll's own \
         file-read-and-match wiring (not merely the pure watch::detect fallback comparison, \
         which alone would suppress this exact shape) is what forced the report; got:\n{out}"`
- `tests/cli.rs:19126-19126` `".rigger/dash.marker"`
- `tests/cli.rs:19148-19148` `".rigger/dash.attempt"`
- `tests/cli.rs:19874-19874` `".rigger"`
- `tests/cli.rs:20689-20689` `".rigger"`
- `tests/cli.rs:21030-21030` `".rigger"`
- `tests/cli.rs:21065-21065` `".rigger"`
- `tests/cli.rs:21140-21140` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:21221-21221` `"the step must record a dash marker at .rigger/dash.marker; stderr:\n{err}"`
- `tests/cli.rs:21235-21235` `"`rigger step` under RIGGER_DASH_PORT={dash_port} must bind its step-path dash at EXACTLY \
         that port and record it in .rigger/dash.marker (proving the override reaches the real \
         bind, not the fixed dash::DEFAULT_PORT); the marker instead recorded {marker_port}"`
- `tests/cli.rs:21296-21296` `".rigger"`
- `tests/cli.rs:21377-21377` `".rigger"`
- `tests/cli.rs:21448-21448` `".rigger"`
- `tests/cli.rs:21810-21810` `".rigger"`
- `tests/cli.rs:21822-21822` `".rigger"`
- `tests/cli.rs:21849-21849` `".rigger"`
- `tests/cli.rs:21877-21877` `".rigger"`
- `tests/cli.rs:21888-21888` `".rigger"`
- `tests/cli.rs:21925-21925` `".rigger"`
- `tests/cli.rs:22014-22014` `"the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}"`
- `tests/cli.rs:22086-22086` `"{root}/.rigger/events.db"`
- `tests/cli.rs:22103-22103` `"{root}/.rigger/events.db"`
- `tests/cli.rs:22703-22703` `"{other_root}/.rigger/tmp"`
- `tests/cli.rs:22843-22843` `"{other_root}/.rigger/tmp"`
- `tests/cli.rs:22978-22978` `"{other_root}/.rigger/tmp"`
- `tests/cli.rs:22985-22985` `"{other_root}/.rigger/events.db"`
- `tests/cli.rs:23364-23364` `"/stale/root/.rigger/events.db"`
- `tests/cli.rs:23385-23385` `"/live/root/.rigger/events.db"`
- `tests/cli.rs:23487-23487` `"the step must record a dash marker at .rigger/dash.marker"`
- `tests/cli.rs:23687-23687` `".rigger"`
- `tests/cli.rs:23869-23869` `".rigger"`
- `tests/cli.rs:23972-23972` `".rigger"`
- `tests/cli.rs:24097-24097` `".rigger"`
- `tests/cli.rs:24754-24754` `".rigger"`
- `tests/cli.rs:24817-24817` `".rigger/agents/rust-engineer.md must pin the mutation ACCOUNTING contract spec 73 \
         criterion 2 owns - a deterministically ordered DecisionMade, one entry per mutant, \
         all five statuses (caught, missed-killed naming the test, missed-justified with a \
         reason, unviable, timeout), the diff base, the mutant total, and the provably-empty \
         no-Rust-file case - so drift in the operator-seeded persona fails this suite instead \
         of silently diverging from the spec it satisfies. Missing fragments: {missing:#?}"`
- `tests/cli.rs:24866-24866` `".rigger/dash.attempt"`
- `tests/cli.rs:24867-24867` `"a real step's own ensure_run_dashboard call must record .rigger/dash.attempt \
         (record_dash_attempt); without it this test cannot exercise the round-8 fact at all"`
- `tests/cli.rs:24924-24924` `".rigger/dash.url"`
- `tests/cli.rs:24932-24932` `".rigger/dash.marker"`
- `tests/cli.rs:24996-24996` `".rigger/dash.marker"`
- `tests/cli.rs:25025-25025` `".rigger/dash.url"`
- `tests/cli.rs:25032-25032` `".rigger/dash.attempt"`
- `tests/cli.rs:25093-25093` `".rigger/dash.marker"`
- `tests/cli.rs:25096-25096` `".rigger/dash.url"`
- `tests/cli.rs:25150-25150` `".rigger/dash.marker"`
- `tests/cli.rs:25172-25172` `".rigger/dash.url"`
- `tests/cli.rs:25177-25177` `".rigger/dash.attempt"`
- `tests/cli.rs:25216-25216` `".rigger/dash.url"`
- `tests/cli.rs:25227-25227` `".rigger/dash.marker"`
- `tests/cli.rs:25263-25263` `".rigger/dash.url"`
- `tests/cli.rs:25271-25271` `".rigger/dash.marker"`
- `tests/cli.rs:25323-25323` `".rigger/dash.url"`
- `tests/cli.rs:25328-25328` `".rigger/dash.marker"`
- `tests/cli.rs:25390-25390` `".rigger/dash.url"`
- `tests/cli.rs:25398-25398` `".rigger/dash.marker"`
- `tests/cli.rs:25651-25651` `".rigger"`
- `tests/cli.rs:26359-26359` `".rigger"`
- `tests/cli.rs:26397-26397` `".rigger"`
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
- `tests/duplication_catalog_contract_periphery.rs:69-69` `".rigger-path string literals"`
- `tests/gate_store_fence_periphery.rs:208-208` `".rigger"`
- `tests/gate_store_fence_periphery.rs:209-209` `".rigger"`
- `tests/gate_store_fence_periphery.rs:214-214` `".rigger"`
- `tests/gate_store_fence_periphery.rs:215-215` `".rigger"`
- `tests/gate_store_fence_periphery.rs:217-217` `".rigger"`
- `tests/gate_store_fence_periphery.rs:222-222` `".rigger"`
- `tests/gate_store_fence_periphery.rs:452-452` `".rigger"`
- `tests/gate_store_fence_periphery.rs:475-475` `".rigger"`
- `tests/gate_store_fence_periphery.rs:510-510` `".rigger"`
- `tests/gate_store_fence_periphery.rs:511-511` `".rigger"`
- `tests/gate_store_fence_periphery.rs:524-524` `".rigger"`
- `tests/gate_store_fence_periphery.rs:527-527` `".rigger"`
- `tests/gate_store_fence_periphery.rs:596-596` `".rigger"`
- `tests/gate_store_fence_periphery.rs:597-597` `".rigger"`
- `tests/gate_store_fence_periphery.rs:614-614` `".rigger"`
- `tests/gate_store_fence_periphery.rs:616-616` `".rigger"`
- `tests/gate_store_fence_periphery.rs:694-694` `".rigger"`
- `tests/gate_store_fence_periphery.rs:695-695` `".rigger"`
- `tests/gate_store_fence_periphery.rs:705-705` `".rigger"`
- `tests/gate_store_fence_periphery.rs:707-707` `".rigger"`
- `tests/gate_store_fence_periphery.rs:808-808` `".rigger"`
- `tests/gate_store_fence_periphery.rs:809-809` `".rigger"`
- `tests/graph_show_periphery.rs:81-81` `".rigger"`
- `tests/graph_show_periphery.rs:134-134` `".rigger"`
- `tests/graph_show_staleness.rs:69-69` `".rigger"`
- `tests/graph_show_staleness.rs:75-75` `".rigger"`
- `tests/graph_show_surface.rs:68-68` `".rigger"`
- `tests/graph_show_surface.rs:150-150` `".rigger"`
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
- `tests/heartbeat_write_read_agree_periphery.rs:414-414` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:425-425` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:510-510` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:521-521` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:593-593` `".rigger"`
- `tests/heartbeat_write_read_agree_periphery.rs:603-603` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:454-454` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:454-454` `"create .rigger"`
- `tests/migration_is_deliberate_periphery.rs:474-474` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:518-518` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:549-549` `".rigger"`
- `tests/migration_is_deliberate_periphery.rs:616-616` `".rigger"`
- `tests/mutation_scratch_persona_encoding_contract_periphery.rs:38-38` `".rigger/agents/rust-engineer.md"`
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
- `tests/reset_build_cache_periphery.rs:46-46` `".rigger"`
- `tests/reset_build_cache_periphery.rs:55-55` `".rigger"`
- `tests/reset_build_cache_periphery.rs:62-62` `".rigger"`
- `tests/reset_build_cache_periphery.rs:66-66` `".rigger"`
- `tests/reset_build_cache_periphery.rs:463-463` `".rigger"`
- `tests/reset_build_cache_periphery.rs:540-540` `".rigger"`
- `tests/reset_derived_compaction.rs:45-45` `".rigger"`
- `tests/reset_derived_compaction.rs:45-45` `"create .rigger"`
- `tests/reset_derived_compaction.rs:62-62` `".rigger"`
- `tests/reset_derived_compaction.rs:76-76` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:606-606` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:606-606` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:611-611` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:627-627` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1359-1359` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:1365-1365` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3290-3290` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3290-3290` `"create .rigger"`
- `tests/reset_derived_compaction_periphery.rs:3304-3304` `".rigger"`
- `tests/reset_derived_compaction_periphery.rs:3339-3339` `".rigger"`
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
- `tests/simplification_audit.rs:2358-2358` `".rigger-path string literals"`
- `tests/simplification_audit.rs:2499-2499` `".rigger"`
- `tests/simplification_audit.rs:2500-2500` `"one .rigger-relative path-composition helper"`
- `tests/simplification_audit.rs:3376-3376` `"| Conductor orchestration: gates, courier, step/run lifecycle | 19 | 8,257 | \
        the `courier_registry_refresh_{boundary,fence,periphery}` trio (3 files) \
        pair together in 6 clusters confined to just themselves (2-5 sites each; \
        excludes the whole-codebase Command::new/`.rigger`-path mandatory-sweep \
        clusters, section 2, that also happen to intersect them) |\n"`
- `tests/simplification_audit.rs:3686-3686` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk: a use case already depends on the wrong \
        concretion, or two independent implementations of one concern can already drift \
        apart silently (section 3's two boundary violations; the one already-drifted \
        `/proc`-reading pair section 2 and section 3 both name). These are live gaps, not \
        just size.\n\
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
- `tests/simplification_audit.rs:3941-3941` `"#### 10. Consolidate the 655 `.rigger`-path string-literal sites (`dup-0052`) - the \
        single largest cluster in the entire catalog by site count\n\n"`
- `tests/simplification_audit.rs:3945-3945` `"- Scope: one `.rigger`-relative path-composition helper (the cluster's own \
        `proposed_home`) every one of the 655 sites routes through instead of building its \
        own literal.\n\
        - Files: spans dozens of files including `src/conductor.rs`, `src/config.rs`, \
        `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, \
        `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, \
        `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list \
        is in the committed `docs/audit/duplication-catalog.json` under `dup-0052` for the \
        follow-up spec to consume directly, not re-enumerated here.\n\
        - Expected line delta: negative - 655 literal compositions collapse toward one \
        helper's call sites; the helper itself is small.\n\
        - Risk: medium - the largest surface-area sweep in this plan by site count, even \
        though each individual site is trivial; needs a mechanical rewrite pass plus a \
        full-suite green run, not hand-editing 655 sites.\n\
        - Unblocks: the biggest single site-count reduction available anywhere in the \
        duplication catalog.\n\n"`
- `tests/simplification_audit.rs:5251-5251` `"fn a() {\n    let _ = \".rigger/tmp\";\n}\n"`
- `tests/simplification_audit.rs:5254-5254` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:57-57` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:76-76` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:98-98` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:177-177` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:297-297` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:308-308` `".rigger"`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:322-322` `".rigger"`
- `tests/step_attention_periphery.rs:161-161` `".rigger"`
- `tests/step_attention_periphery.rs:183-183` `".rigger"`
- `tests/step_attention_periphery.rs:222-222` `".rigger"`
- `tests/step_attention_periphery.rs:493-493` `".rigger"`
- `tests/step_sheds_the_freshen.rs:67-67` `".rigger/grounding"`
- `tests/step_sheds_the_freshen.rs:216-216` `".rigger/symbols/index.json"`
- `tests/step_sheds_the_freshen.rs:217-217` `"the surviving persisted index is the SYMBOL index under .rigger/symbols/; got {}"`
- `tests/step_sheds_the_freshen.rs:282-282` `"constructing the default grounder builds and persists the SYMBOL index under \
         .rigger/symbols/ - the freshen's real target"`
- `tests/store_config.rs:34-34` `".rigger"`
- `tests/store_config.rs:35-35` `"create .rigger"`
- `tests/store_content_identity_periphery.rs:1345-1345` `".rigger"`
- `tests/store_content_identity_periphery.rs:1346-1346` `"create .rigger"`
- `tests/store_content_identity_periphery.rs:1397-1397` `".rigger"`
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
- `tests/worktree_liveness_fence_periphery.rs:312-312` `".rigger"`
- `tests/worktree_liveness_fence_periphery.rs:315-315` `"premise: the default scratch root is `.rigger/tmp` (load-bearing for the worktree \
         paths built below)"`
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

#### `dup-0053` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14285-14305` `render_capped_findings`
- `src/conductor.rs:14527-14546` `render_capped_lessons`
- `src/conductor.rs:16884-16906` `render_capped_lessons_scoped`

#### `dup-0054` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:14308-14375` `findings_prompt_injection_is_capped_under_budget_with_elision_note`
- `src/conductor.rs:16772-16838` `lessons_prompt_injection_is_capped_under_budget_with_elision_note`

#### `dup-0055` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16187-16189` `resolve`
- `src/conductor.rs:30038-30040` `resolve`

#### `dup-0056` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:16576-16616` `a_subgraph_with_no_design_intent_renders_no_design_intent_header`
- `src/conductor.rs:16739-16769` `a_subgraph_with_no_code_definitions_renders_no_code_neighborhood_header`

#### `dup-0057` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17072-17089` `commit_on_unit_branch`
- `src/conductor.rs:17292-17308` `commit_on_named_branch`

#### `dup-0058` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17092-17187` `resume_reuses_a_units_branch_instead_of_reimplementing`
- `src/conductor.rs:19315-19400` `resume_integrates_an_already_approved_unit_without_re_reviewing`
- `src/conductor.rs:19403-19514` `a_failed_unit_is_not_terminal_and_resumes`
- `src/conductor.rs:26341-26444` `a_resumed_reviewed_unit_restores_a_worktree_the_exhaustive_gate_deleted`

#### `dup-0059` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17190-17284` `branch_gc_reclaims_integrated_units_and_retains_escalated_ones_on_resume`
- `src/conductor.rs:17323-17379` `branch_gc_falls_back_to_the_derived_branch_when_unitstarted_recorded_no_branch`
- `src/conductor.rs:17601-17693` `branch_gc_reclaims_every_integrated_unit_in_one_resume_not_just_the_first`
- `src/conductor.rs:17696-17777` `branch_gc_fences_reclaim_behind_an_in_flight_straggler_spawn_and_reclaims_once_it_answers`

#### `dup-0060` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17780-17847` `gc_integrated_branches_logged_prints_kept_evidence_for_an_in_flight_straggler_spawn`
- `src/conductor.rs:17952-18035` `gc_integrated_branches_logged_stays_silent_for_an_already_gone_worktree_but_still_reclaims_an_orphaned_branch`

#### `dup-0061` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:17850-17949` `gc_integrated_branches_logged_prints_removing_evidence_for_a_terminal_spawns_decision`
- `src/conductor.rs:18038-18141` `gc_integrated_branches_logged_does_not_repeat_removing_evidence_once_the_real_removal_already_happened`

#### `dup-0062` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:18897-18942` `a_gating_spawn_that_returns_a_reject_verdict_line_is_a_normal_reject_even_if_it_emitted_approve`
- `src/conductor.rs:18945-18989` `a_gating_spawn_with_no_verdict_line_and_no_emitted_approve_is_an_ordinary_reject`

#### `dup-0063` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19839-19881` `scope_creep_refuses_a_criterionless_proposed_unit`
- `src/conductor.rs:24404-24445` `planner_covering_every_criterion_passes`

#### `dup-0064` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:19920-19986` `adversary_runs_between_the_lenses_and_the_adjudicator`
- `src/conductor.rs:20042-20135` `unit_reviews_itself_within_its_own_lifecycle`

#### `dup-0065` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:20418-20478` `a_low_risk_unit_skips_the_adversary_and_extra_lens`
- `src/conductor.rs:20481-20551` `a_high_risk_unit_runs_the_full_panel_and_logs_the_routing`
- `src/conductor.rs:20572-20650` `a_zero_grounding_unit_fails_safe_to_the_full_panel_and_logs_it`
- `src/conductor.rs:20765-20829` `a_stage_level_tiers_policy_routes_the_unit_by_risk`

#### `dup-0066` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:22401-22463` `planner_proposed_unit_inherits_the_default_review_panel`
- `src/conductor.rs:22466-22561` `a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents`

#### `dup-0067` (near, 5 sites)

Proposed home: `conductor::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:22564-22636` `per_unit_adjudicator_reject_blocks_integration_and_escalates`
- `src/conductor.rs:22963-23036` `approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage`
- `src/conductor.rs:23132-23198` `approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage`
- `src/conductor.rs:25082-25129` `on_pass_none_runs_gates_but_does_not_integrate`
- `src/conductor.rs:28245-28290` `unparseable_adjudicator_output_blocks_integration`

#### `dup-0068` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:22735-22832` `a_higher_max_retries_gives_more_attempts_before_escalation`
- `src/conductor.rs:22749-22795` `escalation_run`

#### `dup-0069` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23201-23235` `mid_spawn_crash_escalates_without_aborting_the_run`
- `src/conductor.rs:23238-23289` `a_newly_escalated_unit_stamps_an_attention_entry`

#### `dup-0070` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23292-23342` `a_budget_halt_stamps_an_attention_entry`
- `src/conductor.rs:24105-24143` `a_budget_halt_surfaces_its_reason_on_the_run_state`

#### `dup-0071` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:23437-23541` `a_delayed_budget_halt_after_a_dependency_unlocks_still_stamps`
- `src/conductor.rs:23618-23699` `an_escalation_does_not_restamp_attention_on_a_resumed_process`
- `src/conductor.rs:23738-23862` `a_second_failure_recurs_and_a_third_also_stalls_the_frontier`

#### `dup-0072` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:24005-24056` `budget_breaker_stops_the_run_after_the_first_wave`
- `src/conductor.rs:24059-24102` `budget_exhaustion_aborts_the_task`
- `src/conductor.rs:24522-24581` `manual_stage_pauses_while_an_auto_stage_integrates`

#### `dup-0073` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:24181-24249` `the_spawn_budget_folds_from_recorded_spawn_requests_across_steps`
- `src/conductor.rs:24252-24293` `the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn`

#### `dup-0074` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:24584-24627` `isolation_none_agent_gets_no_worktree_even_with_a_repo`
- `src/conductor.rs:24630-24663` `spawn_opts_isolation_is_set_for_a_worktree_agent`
- `src/conductor.rs:24666-24705` `a_spawned_implementers_title_is_the_unit_criterion`

#### `dup-0075` (near, 4 sites)

Proposed home: `conductor::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:24712-24760` `the_adversarys_spawn_is_stamped_with_the_units_lens_roster`
- `src/conductor.rs:24766-24815` `the_adjudicators_spawn_is_stamped_with_lenses_plus_adversary`
- `src/conductor.rs:24820-24867` `a_panel_with_no_adversary_never_fabricates_one_in_the_adjudicators_roster`
- `src/conductor.rs:24953-25010` `the_fan_out_review_loops_adversary_and_adjudicator_spawns_are_stamped_with_the_routed_roster`

#### `dup-0076` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:25260-25262` `envs`
- `src/conductor.rs:29197-29199` `build_envs`

#### `dup-0077` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:25663-25769` `a_parked_review_spawn_keeps_the_unit_worktree_registered_and_its_cache`
- `src/conductor.rs:25772-25864` `review_unit_restores_a_worktree_a_gate_deleted_out_of_band`
- `src/conductor.rs:25867-25969` `a_second_step_restores_a_still_parked_units_worktree_deleted_out_of_band`

#### `dup-0078` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:26271-26338` `failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`
- `src/conductor.rs:26873-26944` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`

#### `dup-0079` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27207-27250` `agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path`
- `src/conductor.rs:27253-27294` `standalone_review_stage_still_takes_the_fan_out_path`

#### `dup-0080` (near, 6 sites)

Proposed home: `conductor::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:27412-27465` `a_parked_lens_keeps_the_standalone_review_stages_worktree`
- `src/conductor.rs:27468-27564` `a_parked_lens_keeps_the_review_worktree_even_beside_a_lower_indexed_sibling_crash`
- `src/conductor.rs:27567-27660` `a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt`
- `src/conductor.rs:27663-27748` `the_swap_to_front_prioritizes_a_genuine_error_at_a_non_zero_chunk_index`
- `src/conductor.rs:27751-27838` `a_budget_refused_lens_beside_a_genuinely_crashing_sibling_in_one_chunk`
- `src/conductor.rs:27841-27908` `a_budget_refused_standalone_review_spawn_keeps_its_worktree`

#### `dup-0081` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:28357-28440` `a_flaky_gate_rerun_is_a_pass_with_warning_that_never_demotes`
- `src/conductor.rs:28443-28513` `a_product_gate_failure_is_not_rerun_and_demotes_as_before`

#### `dup-0082` (exact, 2 sites)

Proposed home: `conductor::recording_runner`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29176-29181` `materializing`
- `src/conductor.rs:29185-29190` `deleting_worktree`

#### `dup-0083` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/conductor.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29287-29293` `attempt_of`
- `src/spawn.rs:194-200` `attempt_of`

#### `dup-0084` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29405-29471` `a_matching_input_digest_answers_a_gate_as_a_logged_cache_hit_citing_the_prior_green`
- `src/conductor.rs:29519-29575` `a_red_verdict_is_never_cache_answered_and_the_gate_re_runs`

#### `dup-0085` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:29711-29784` `a_structural_grounder_records_the_audit_and_routes_full_on_a_beyond_cap_high_risk_file`
- `src/conductor.rs:30324-30388` `speculation_over_a_structural_grounder_records_the_audit_and_routes_full`

#### `dup-0086` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:30640-30681` `spawn`
- `src/conductor.rs:31520-31557` `spawn`
- `src/conductor.rs:31655-31700` `spawn`

#### `dup-0087` (exact, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:31561-31644` `a_compensated_unit_re_gates_its_re_implemented_tree_not_the_condemned_verdict`
- `src/conductor.rs:31704-31802` `a_compensated_unit_that_remediated_before_integrating_re_gates_at_a_fresh_key`

#### `dup-0088` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:32309-32377` `a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done`
- `src/conductor.rs:32414-32485` `a_default_infra_fault_at_a_deferred_gate_does_not_demote`

#### `dup-0089` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:32488-32592` `a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events`
- `src/conductor.rs:32595-32680` `a_re_step_replays_a_recorded_deferred_gate_without_re_running_it`

#### `dup-0090` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:33987-34062` `an_empty_accounting_sdet_author_result_advances_the_build_to_commit_gates_and_integrate`
- `src/conductor.rs:34065-34129` `an_absent_sdet_author_is_a_clean_no_op_and_the_build_still_integrates`
- `src/conductor.rs:34132-34207` `a_crashed_sdet_author_spawn_does_not_block_the_build_the_lifecycle_proceeds`

#### `dup-0091` (exact, 2 sites)

Proposed home: `conductor::critique_driver`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34400-34405` `rejecting`
- `src/conductor.rs:34408-34413` `always_rejecting`

#### `dup-0092` (near, 2 sites)

Proposed home: `conductor::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34607-34644` `a_rule_7_or_8_defect_rejects_then_releases_on_the_revision`
- `src/conductor.rs:34647-34698` `a_clean_decomposition_approves_and_releases_the_fan_out`

#### `dup-0093` (near, 3 sites)

Proposed home: `conductor::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/conductor.rs:34923-35021` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_escalated`
- `src/conductor.rs:35024-35080` `an_approved_gate_releases_planner_proposed_units_not_only_baselines`
- `src/conductor.rs:35203-35274` `a_resumed_step_holds_the_fan_out_while_the_plan_critique_gate_is_mid_review`

#### `dup-0094` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/failure.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:205-207` `is_empty`
- `src/failure.rs:168-170` `is_any`
- `src/main.rs:9731-9736` `is_empty`

#### `dup-0095` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:918-938` `read_store_config`
- `src/config.rs:975-990` `read_scratch_defaults`

#### `dup-0096` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, tests/no_os_kill_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1589-1591` `is_word_byte`
- `tests/no_os_kill_audit.rs:52-54` `is_word_char`
- `tests/simplification_audit.rs:181-183` `is_ident_char`

#### `dup-0097` (near, 3 sites)

Proposed home: `config::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1750-1789` `verdict_line_lint_passes_an_unrelated_same_sentence_emit_before_a_verdict_output_clause`
- `src/config.rs:1801-1833` `verdict_line_lint_passes_a_determiner_verdict_when_a_payload_noun_sits_in_the_span`
- `src/config.rs:1846-1886` `verdict_line_lint_passes_a_determiner_verdict_when_an_unrelated_emit_example_brace_precedes_it`

#### `dup-0098` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/main.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:1922-1955` `literal_is_emit_payload_binds_only_an_abutting_payload_or_emit_word`
- `src/main.rs:15758-15764` `is_uuid8_accepts_exactly_eight_hex_digits`
- `tests/simplification_audit.rs:5258-5266` `looks_error_shaping_matches_error_and_underscore_bounded_err_but_not_an_incidental_substring`

#### `dup-0099` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2109-2116` `parses_agent_frontmatter_and_body`
- `src/config.rs:2124-2133` `model_ladder_parses_from_frontmatter`

#### `dup-0100` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/config.rs, src/main.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2119-2121` `rejects_missing_frontmatter`
- `src/main.rs:15069-15071` `dirty_tracked_paths_on_a_clean_tree_is_empty`
- `src/spec.rs:895-897` `empty_when_no_criteria`

#### `dup-0101` (near, 6 sites)

Proposed home: `config::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2214-2223` `defaults_max_wall_clock_parses_and_is_zero_when_absent`
- `src/config.rs:2733-2749` `max_retries_parses_from_defaults_and_defaults_to_zero_when_absent`
- `src/config.rs:2998-3011` `build_config_parses_wrapper_and_cache_dir_and_defaults_when_omitted`
- `src/config.rs:3020-3035` `build_config_parses_jobs_and_defaults_to_zero_when_omitted`
- `src/config.rs:3044-3059` `build_config_parses_max_concurrent_defaulting_to_four_when_omitted`
- `src/config.rs:3107-3116` `build_config_parses_mutation_and_defaults_to_off_when_omitted`

#### `dup-0102` (near, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2469-2487` `validate_catches_unknown_ref`
- `src/config.rs:3278-3306` `validate_catches_cycle`

#### `dup-0103` (near, 3 sites)

Proposed home: `config::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:2579-2602` `validate_catches_an_unknown_light_panel_agent`
- `src/config.rs:2605-2629` `validate_rejects_a_light_panel_with_no_adjudicator`
- `src/config.rs:2660-2687` `validate_rejects_a_tiers_policy_on_a_full_panel_with_no_adjudicator`

#### `dup-0104` (exact, 2 sites)

Proposed home: `config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/config.rs:3091-3100` `validate_accepts_auto_and_off_wrapper_regardless_of_path`
- `src/config.rs:3125-3134` `validate_accepts_off_mutation_regardless_of_path`

#### `dup-0105` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/mod.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/mod.rs:475-477` `is_false`
- `src/dash.rs:1151-1153` `is_not_back`
- `src/dash.rs:1158-1160` `is_not_shared`

#### `dup-0106` (semantic, 46 sites)

Proposed home: `one sqlite-connection-opening adapter function every caller is injected with`

mandatory sweep: sqlite Connection::open call sites - 46 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/contextgraph/sqlite.rs:132-132` `Connection::open`
- `src/contextgraph/sqlite.rs:6759-6759` `Connection::open`
- `src/contextgraph/sqlite.rs:7569-7569` `Connection::open`
- `src/eventstore/sqlite.rs:159-159` `Connection::open`
- `src/eventstore/sqlite.rs:2792-2792` `Connection::open`
- `src/eventstore/sqlite.rs:3484-3484` `Connection::open`
- `src/eventstore/sqlite.rs:3576-3576` `Connection::open`
- `src/eventstore/sqlite.rs:3833-3833` `Connection::open_with_flags`
- `src/eventstore/sqlite.rs:3851-3851` `Connection::open`
- `src/eventstore/sqlite.rs:3865-3865` `Connection::open`
- `src/main.rs:23983-23983` `Connection::open`
- `tests/cli.rs:901-901` `Connection::open`
- `tests/cli.rs:970-970` `Connection::open`
- `tests/cli.rs:1066-1066` `Connection::open`
- `tests/cli.rs:1159-1159` `Connection::open`
- `tests/cli.rs:1214-1214` `Connection::open`
- `tests/cli.rs:9652-9652` `Connection::open`
- `tests/cli.rs:15704-15704` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:69-69` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:165-165` `Connection::open`
- `tests/graph_additive_indexes_persist.rs:189-189` `Connection::open`
- `tests/heartbeat_write_read_agree_periphery.rs:215-215` `Connection::open`
- `tests/reset_derived_compaction.rs:132-132` `Connection::open`
- `tests/reset_derived_compaction.rs:306-306` `Connection::open`
- `tests/reset_derived_compaction.rs:571-571` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:117-117` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:568-568` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:1372-1372` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:2857-2857` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3062-3062` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:3938-3938` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4029-4029` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4038-4038` `Connection::open`
- `tests/reset_derived_compaction_periphery.rs:4646-4646` `Connection::open`
- `tests/reset_derived_live_writer_guard_periphery.rs:128-128` `Connection::open`
- `tests/reset_menu.rs:175-175` `Connection::open`
- `tests/reset_menu.rs:179-179` `Connection::open`
- `tests/reset_menu_identity_migration_periphery.rs:201-201` `Connection::open`
- `tests/store_append_order_periphery.rs:58-58` `Connection::open`
- `tests/store_content_identity_periphery.rs:905-905` `Connection::open`
- `tests/store_content_identity_periphery.rs:1753-1753` `Connection::open`
- `tests/store_content_identity_periphery.rs:1779-1779` `Connection::open`
- `tests/store_content_identity_periphery.rs:1788-1788` `Connection::open`
- `tests/store_content_identity_periphery.rs:2088-2088` `Connection::open`
- `tests/watchdog_cli_periphery.rs:236-236` `Connection::open`
- `tests/watchdog_cli_periphery.rs:272-272` `Connection::open`

#### `dup-0107` (near, 2 sites)

Proposed home: `sqlite::projector`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:541-656` `calls_down`
- `src/contextgraph/sqlite.rs:692-801` `calls_up`

#### `dup-0108` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/contextgraph/sqlite.rs, src/eventstore/kurrentdb.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:918-922` `to_nanos`
- `src/eventstore/kurrentdb.rs:115-119` `to_nanos`
- `src/eventstore/sqlite.rs:1468-1472` `to_nanos`

#### `dup-0109` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/contextgraph/sqlite.rs, src/spawn.rs, tests/no_os_kill_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:1917-1919` `code_entity_id`
- `src/spawn.rs:441-443` `what`
- `tests/no_os_kill_audit.rs:45-47` `join`

#### `dup-0110` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, src/dash.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:1925-1930` `name_suffix`
- `src/dash.rs:1696-1701` `name_suffix`

#### `dup-0111` (exact, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:1960-1966` `tier_rank`
- `src/contextgraph/sqlite.rs:1972-1978` `tier_floor_rank`

#### `dup-0112` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2045-2070` `calls_out`
- `src/contextgraph/sqlite.rs:2103-2128` `callers_direct`
- `src/contextgraph/sqlite.rs:2139-2167` `callers_via_bare`

#### `dup-0113` (near, 5 sites)

Proposed home: `a new shared module (sites span 4 files: src/contextgraph/sqlite.rs, src/dash.rs, tests/calls_down_execution_path_periphery.rs, tests/graph_fold_dedup_live_only_scoping.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2806-2820` `apply_decision`
- `src/contextgraph/sqlite.rs:4501-4508` `apply_batch_ref_caller`
- `src/dash.rs:9906-9913` `apply_call`
- `tests/calls_down_execution_path_periphery.rs:80-87` `apply_call`
- `tests/graph_fold_dedup_live_only_scoping.rs:38-45` `apply_decision`

#### `dup-0114` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:2895-2908` `subgraph_finds_the_governing_decision`
- `src/contextgraph/sqlite.rs:7926-7946` `recording_proof_never_wipes_the_entitys_own_name_kind_and_line_attrs`

#### `dup-0115` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/contextgraph/sqlite.rs, tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3039-3047` `apply_governs_at`
- `tests/graph_fold_dedup_live_edge.rs:38-46` `apply_governs`
- `tests/graph_rebuild_collapses_dupes.rs:40-48` `apply_governs`

#### `dup-0116` (near, 9 sites)

Proposed home: `a new shared module (sites span 6 files: src/contextgraph/sqlite.rs, src/dash.rs, tests/calls_down_execution_path_periphery.rs, tests/dash_calls_route_periphery.rs, tests/graph_superseded_prune.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3330-3348` `apply_code_entity`
- `src/contextgraph/sqlite.rs:3404-3423` `apply_community`
- `src/contextgraph/sqlite.rs:4475-4486` `apply_batch_def`
- `src/contextgraph/sqlite.rs:4769-4789` `apply_batch_def_at`
- `src/dash.rs:9894-9905` `apply_def`
- `tests/calls_down_execution_path_periphery.rs:63-74` `apply_def`
- `tests/dash_calls_route_periphery.rs:741-751` `apply_def`
- `tests/graph_superseded_prune.rs:36-48` `apply_def`
- `tests/reset_menu_previews_periphery.rs:55-67` `apply_def`

#### `dup-0117` (near, 9 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/dash_calls_route_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:3350-3355` `apply_edge_inferred`
- `src/contextgraph/sqlite.rs:3361-3366` `apply_edge_inferred_evidence`
- `src/contextgraph/sqlite.rs:3394-3400` `apply_ref_caller`
- `src/contextgraph/sqlite.rs:4153-4161` `apply_doc_concept`
- `src/contextgraph/sqlite.rs:4264-4272` `apply_doc_link`
- `src/contextgraph/sqlite.rs:4490-4496` `apply_batch_ref`
- `src/contextgraph/sqlite.rs:6104-6112` `apply_unit_integrated`
- `src/contextgraph/sqlite.rs:7375-7385` `apply_def`
- `tests/dash_calls_route_periphery.rs:755-761` `apply_call`

#### `dup-0118` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:4514-4528` `edges_from`
- `src/contextgraph/sqlite.rs:6832-6850` `edges_touching`

#### `dup-0119` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:5192-5332` `calls_down_walks_the_execution_path_as_a_layered_deduped_dag_with_a_back_edge`
- `src/contextgraph/sqlite.rs:5530-5720` `calls_up_walks_the_call_sites_as_a_layered_deduped_dag_and_lists_referenced_but_not_called`

#### `dup-0120` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:5837-5870` `decision_fold_projects_no_agent_node_or_decided_edge`
- `src/contextgraph/sqlite.rs:5924-5952` `review_finding_projects_no_raised_edge_even_with_an_event_actor`

#### `dup-0121` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6647-6656` `edge_projects`
- `src/contextgraph/sqlite.rs:7734-7746` `index_names`

#### `dup-0122` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:6659-6741` `every_node_and_edge_carries_the_projects_scope_on_fold`
- `src/contextgraph/sqlite.rs:6853-6944` `prune_is_project_scoped_leaving_another_projects_same_id_node_intact`

#### `dup-0123` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7388-7393` `apply_ref`
- `tests/calls_down_execution_path_periphery.rs:94-99` `apply_ref`

#### `dup-0124` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, tests/code_ingest_events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7473-7514` `the_cross_file_inferred_tier_is_order_independent`
- `src/contextgraph/sqlite.rs:7517-7532` `the_definition_upgrade_never_demotes_a_same_file_extracted_reference`
- `tests/code_ingest_events.rs:1013-1045` `a_definition_upgrades_only_the_exact_name_cross_file_reference_never_a_substring`

#### `dup-0125` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/contextgraph/sqlite.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7643-7645` `edge_desc`
- `src/main.rs:7925-7931` `runs_menu_line`

#### `dup-0126` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7844-7863` `a_same_file_test_reference_increments_proven_by_and_records_its_evidence`
- `src/contextgraph/sqlite.rs:7866-7889` `two_test_references_accumulate_proven_by_to_2_with_both_evidence_entries`

#### `dup-0127` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/contextgraph/sqlite.rs:7968-7999` `an_unresolvable_test_reference_is_staged_and_reconciled_once_its_definition_later_folds`
- `src/contextgraph/sqlite.rs:8244-8275` `the_empty_boundary_sentinel_never_resolves_records_or_stages_anything_for_its_empty_name`

#### `dup-0128` (semantic, 59 sites)

Proposed home: `src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - see the report's worked example)`

mandatory sweep: /proc-path string literals - 59 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

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
- `src/main.rs:13104-13104` `"/proc"`
- `src/main.rs:13208-13208` `"/proc"`
- `src/main.rs:23188-23188` `"/proc/{pid}/stat"`
- `src/main.rs:23189-23189` `"read /proc/{pid}/stat: {e}"`
- `src/main.rs:23192-23192` `"/proc stat has a parenthesised comm field"`
- `src/main.rs:23197-23197` `"/proc stat has a pgrp field after comm"`
- `src/reap.rs:109-109` `"/proc"`
- `src/reap.rs:191-191` `"/proc/{pid}/stat"`
- `src/reap.rs:202-202` `"/proc/{pid}/status"`
- `src/reap.rs:252-252` `"/proc/{}/cwd"`
- `tests/cli.rs:21323-21323` `"/proc"`
- `tests/cli.rs:23420-23420` `"/proc/{pid}/stat"`
- `tests/cli.rs:23421-23421` `"read /proc/{pid}/stat: {e}"`
- `tests/cli.rs:23424-23424` `"/proc stat has a parenthesised comm field"`
- `tests/cli.rs:23429-23429` `"/proc stat has a pgrp field after comm"`
- `tests/cli.rs:26428-26428` `"/proc"`
- `tests/cli.rs:26513-26513` `"/proc"`
- `tests/cli.rs:26560-26560` `"/proc/{holder_pid}/stat"`
- `tests/cli.rs:26575-26575` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:26641-26641` `"/proc"`
- `tests/cli.rs:26664-26664` `"/proc"`
- `tests/cli.rs:26693-26693` `"/proc"`
- `tests/cli.rs:26743-26743` `"/proc/{holder_pid}/stat"`
- `tests/cli.rs:26758-26758` `"the holder pid {holder_pid} never reached the STOPPED (T) state in /proc"`
- `tests/cli.rs:26832-26832` `"/proc"`
- `tests/cli.rs:26858-26858` `"/proc"`
- `tests/cli.rs:26956-26956` `"/proc"`
- `tests/cli.rs:26994-26994` `"held_port_holder's message half and describe_held_port_if_confirmed's own return \
             must agree (scheduler-state letter normalized away since each call independently \
             re-reads /proc and can observe a state flap) - they are documented as sharing one \
             discovery"`
- `tests/cli.rs:27013-27013` `"/proc"`
- `tests/duplication_catalog_contract_periphery.rs:67-67` `"/proc-path string literals"`
- `tests/simplification_audit.rs:2356-2356` `"/proc-path string literals"`
- `tests/simplification_audit.rs:2487-2487` `"/proc"`
- `tests/simplification_audit.rs:2488-2488` `"src/reap.rs as the one /proc-reading module (dash.rs's own /proc readers already \
             duplicate reap.rs's field-after-the-comm's-closing-paren /proc/<pid>/stat parse - \
             see the report's worked example)"`
- `tests/simplification_audit.rs:2528-2528` `"/proc"`
- `tests/simplification_audit.rs:2750-2750` `"/proc/<pid>/stat or /proc/<pid>/status field-extraction functions"`
- `tests/simplification_audit.rs:2752-2752` `"src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning \
             whichever field each caller needs, so dash.rs::process_state and \
             reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split"`
- `tests/simplification_audit.rs:2956-2956` `"Two real recall gaps surfaced this way and were closed by widening the mechanical \
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
- `tests/simplification_audit.rs:3199-3199` `"A second mutation authority for one domain: the one previously-known \
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
- `tests/simplification_audit.rs:3686-3686` `"Largest risk-reduction first is read as six tiers, ranked by the KIND of risk \
        each entry retires, highest first:\n\n\
        1. Tier 1 - active correctness risk: a use case already depends on the wrong \
        concretion, or two independent implementations of one concern can already drift \
        apart silently (section 3's two boundary violations; the one already-drifted \
        `/proc`-reading pair section 2 and section 3 both name). These are live gaps, not \
        just size.\n\
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
- `tests/simplification_audit.rs:3769-3769` `"#### 3. Retire the duplicate `/proc`-reading authority (`dup-0128` + `dup-0129`)\n\n"`
- `tests/simplification_audit.rs:3772-3772` `"- Scope: `src/dash.rs::process_state` (`src/dash.rs:499-507`) and \
        `src/main.rs::pgid_of` (`src/main.rs:23064-23077`) each independently re-derive \
        `/proc/<pid>/stat` and `/proc/<pid>/status` fields that `src/reap.rs` \
        (`pid_starttime`/`read_ppid`, `src/reap.rs:190-207`) already parses - the exact \
        \"second mutation authority\" example spec 85's own Goal names and spec 62's \
        capstone previously caught (`dup-0129`, 14 sites: `src/dash.rs`, `src/main.rs`, \
        `src/reap.rs`, `tests/cli.rs`), plus 59 raw `/proc`-path string literals scattered \
        across `src/dash.rs`, `src/main.rs`, `src/reap.rs` and three test files with no \
        shared composer (`dup-0128`). Both clusters' own `proposed_home` agree: `src/reap.rs` \
        becomes the one `/proc`-reading module; `dash.rs` and `main.rs` call it instead of \
        re-parsing. NOT SYMMETRIC: `process_state` is reachable from `dash`'s own always-on \
        production server, so it is the actual active-correctness risk this tier-1 placement \
        is about; `pgid_of` sits inside `main.rs`'s `mod tests` (opened at `src/main.rs:12650`) \
        and is called only by `#[test]` fns, so on its own it earns no tier-1 placement - it \
        rides in this same item only because it shares `dup-0128`/`dup-0129`'s one root cause \
        and one proposed fix with `process_state`, not because retiring it retires any live \
        risk of its own.\n\
        - Files: `src/dash.rs`, `src/main.rs`, `src/reap.rs`, `tests/cli.rs` (`proc_pgid_of`, \
        `tests/cli.rs:23419-23432`, re-points at the same call).\n\
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
- `tests/simplification_audit.rs:5136-5136` `"fn state_of(pid: u32) -> Option<char> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().next()?.chars().next()\n}\n"`
- `tests/simplification_audit.rs:5141-5141` `"fn starttime_of(pid: u32) -> Option<u64> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:5146-5146` `"fn ppid_of(pid: u32) -> Option<u32> {\n    let stat = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()\n}\n"`
- `tests/simplification_audit.rs:5237-5237` `"fn a() {\n    let _ = std::fs::read_to_string(\"/proc/1/stat\");\n    let _ = \"hello\";\n}\n"`
- `tests/simplification_audit.rs:5240-5240` `"/proc"`
- `tests/simplification_audit.rs:5242-5242` `"/proc"`
- `tests/simplification_audit.rs:5306-5306` `"fn state_of(pid: u32) -> Option<char> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/stat\")).ok()?;\n    s.chars().next()\n}\nfn ppid_of(pid: u32) -> Option<u32> {\n    let s = std::fs::read_to_string(format!(\"/proc/{pid}/status\")).ok()?;\n    s.parse().ok()\n}\nfn unrelated() -> u32 {\n    1\n}\n"`

#### `dup-0129` (semantic, 14 sites)

Proposed home: `src/reap.rs as the one /proc/<pid>/stat and /proc/<pid>/status parser, returning whichever field each caller needs, so dash.rs::process_state and reap.rs::pid_starttime/read_ppid stop each re-deriving the pid(comm)state... split`

mandatory sweep: /proc/<pid>/stat or /proc/<pid>/status field-extraction functions - 14 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:499-507` `process_state`
- `src/main.rs:23187-23200` `pgid_of`
- `src/reap.rs:190-197` `pid_starttime`
- `src/reap.rs:201-207` `read_ppid`
- `tests/cli.rs:23419-23432` `proc_pgid_of`
- `tests/cli.rs:26508-26608` `cmd_dash_gives_the_stopped_listener_diagnosis_naming_resume_or_kill`
- `tests/cli.rs:26687-26802` `step_names_the_stopped_holder_when_the_step_paths_own_auto_start_hits_the_predecessor_scenario`
- `tests/simplification_audit.rs:2477-2508` `build_sweep_clusters`
- `tests/simplification_audit.rs:2747-2768` `build_extra_semantic_clusters`
- `tests/simplification_audit.rs:2908-3013` `render_adversarial_sample`
- `tests/simplification_audit.rs:3665-4150` `render_section_6`
- `tests/simplification_audit.rs:5128-5154` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:5232-5243` `proc_literal_sweep_finds_a_proc_path_string_and_ignores_an_unrelated_one`
- `tests/simplification_audit.rs:5300-5313` `proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn`

#### `dup-0130` (exact, 3 sites)

Proposed home: `dash::buckets`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:1993-1999` `underived_message`
- `src/dash.rs:2010-2016` `no_membership_message`
- `src/dash.rs:2022-2028` `label_kind`

#### `dup-0131` (exact, 2 sites)

Proposed home: `dash::response`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:4371-4377` `html`
- `src/dash.rs:4378-4384` `json`

#### `dup-0132` (semantic, 3 sites)

Proposed home: `dash::response - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/dash.rs:4371-4377` `html`
- `src/dash.rs:4378-4384` `json`
- `src/dash.rs:4385-4391` `text`

#### `dup-0133` (near, 5 sites)

Proposed home: `dash::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:5468-5484` `dash_serving_on_is_false_for_a_non_dash_listener`
- `src/dash.rs:5625-5645` `dash_serving_pid_on_reports_the_pid_a_real_dash_response_names`
- `src/dash.rs:5667-5690` `dash_serving_pid_on_assembles_a_head_that_spans_many_read_calls`
- `src/dash.rs:5697-5712` `dash_serving_pid_on_is_none_for_a_non_dash_listener`
- `src/dash.rs:5740-5760` `dash_serving_pid_on_is_none_when_the_pid_header_value_is_not_a_number`

#### `dup-0134` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:5495-5540` `dash_serving_on_is_bounded_against_a_byte_dribbling_holder`
- `src/dash.rs:5563-5614` `dash_serving_on_recognizes_the_header_fast_even_if_the_holder_never_finishes_the_block`

#### `dup-0135` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_decisions_progressive_disclosure.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:6024-6088` `the_decision_history_renders_each_decision_as_a_native_details_with_preview_and_full_body`
- `tests/dash_decisions_progressive_disclosure.rs:134-212` `the_served_root_page_ships_the_decisions_progressive_disclosure_region`

#### `dup-0136` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:7598-7632` `tiered_chain_graph`
- `tests/dash_kg_graph_route.rs:39-69` `fixture_graph`

#### `dup-0137` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:8088-8113` `star_graph`
- `tests/dash_kg_graph_route.rs:632-657` `star_graph`

#### `dup-0138` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_kg_graph_route.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:8297-8317` `chain_graph_local`
- `tests/dash_kg_graph_route.rs:75-95` `chain_graph`

#### `dup-0139` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9420-9438` `describe_held_port_names_this_process_when_it_holds_the_port_itself`
- `src/dash.rs:9453-9469` `describe_held_port_if_confirmed_names_the_holder_when_independently_confirmed`

#### `dup-0140` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_calls_route_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9862-9875` `cedge`
- `tests/dash_calls_route_periphery.rs:85-98` `calls_edge`

#### `dup-0141` (exact, 12 sites)

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

#### `dup-0142` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/calls_down_execution_path_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:9885-9887` `layer_of`
- `tests/calls_down_execution_path_periphery.rs:120-122` `layer_of`

#### `dup-0143` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10250-10260` `node`
- `tests/rationale_overlay_seam.rs:30-40` `node`

#### `dup-0144` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/dash_graph_exploration_overview.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10277-10287` `edge`
- `tests/dash_graph_exploration_overview.rs:61-71` `edge`

#### `dup-0145` (exact, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10338-10340` `ids`
- `src/dash.rs:10341-10343` `kinds`

#### `dup-0146` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10543-10567` `an_absent_explain_leaves_the_graph_route_unchanged`
- `src/dash.rs:10832-10852` `a_cluster_drill_carries_no_memory_field`
- `tests/metadata_card_periphery.rs:350-367` `the_served_route_degrades_gracefully_for_an_unknown_card_subject`

#### `dup-0147` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/dash.rs, tests/metadata_card_periphery.rs, tests/subject_view_memory_rail_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10581-10590` `node`
- `src/dash.rs:10868-10877` `node`
- `tests/metadata_card_periphery.rs:41-50` `node`
- `tests/subject_view_memory_rail_contract.rs:37-46` `node`

#### `dup-0148` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/dash.rs, tests/metadata_card_periphery.rs, tests/subject_view_memory_rail_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:10609-10634` `subject_graph`
- `src/dash.rs:10896-10931` `card_graph`
- `tests/metadata_card_periphery.rs:69-105` `fixture_graph`
- `tests/subject_view_memory_rail_contract.rs:64-89` `subject_graph`

#### `dup-0149` (near, 2 sites)

Proposed home: `dash::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:11045-11058` `card_of_a_file_reports_no_proof_of_its_own`
- `src/dash.rs:11065-11075` `card_tolerates_a_malformed_proof_evidence_attr`

#### `dup-0150` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: src/dash.rs, tests/metadata_card_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/dash.rs:11197-11239` `the_card_route_serves_a_known_subjects_card_and_null_for_an_unknown_one`
- `tests/metadata_card_periphery.rs:124-171` `the_served_route_carries_a_code_subjects_card`
- `tests/metadata_card_periphery.rs:179-205` `the_served_route_omits_community_and_line_keys_for_a_membership_less_entity`
- `tests/metadata_card_periphery.rs:374-400` `the_card_param_takes_precedence_over_a_stray_seed_or_lens_param`

#### `dup-0151` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/distiller.rs, src/main.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:44-53` `fnv1a_64`
- `src/main.rs:1008-1017` `fnv1a_64`
- `src/playbooks.rs:36-45` `fnv1a_64`

#### `dup-0152` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:211-223` `render`
- `src/playbooks.rs:125-136` `render`

#### `dup-0153` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:231-244` `rebuild`
- `src/playbooks.rs:143-156` `rebuild`

#### `dup-0154` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/distiller.rs, src/playbooks.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/distiller.rs:259-269` `decision`
- `src/distiller.rs:271-281` `finding`
- `src/playbooks.rs:163-173` `lesson`

#### `dup-0155` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:89-107` `render_using_rigger_skill`
- `src/docs.rs:111-122` `render_handbook_discipline`

#### `dup-0156` (near, 2 sites)

Proposed home: `docs::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:430-432` `render_planning_a_spec_skill`
- `src/docs.rs:601-603` `render_planning_field_guide`

#### `dup-0157` (near, 7 sites)

Proposed home: `docs::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/docs.rs:608-687` `render_reset_store_skill`
- `src/docs.rs:692-737` `render_build_graph_skill`
- `src/docs.rs:742-786` `render_reindex_skill`
- `src/docs.rs:791-845` `render_resume_a_run_skill`
- `src/docs.rs:852-902` `render_handle_an_escalation_skill`
- `src/docs.rs:1007-1080` `render_restore_the_dash_skill`
- `src/docs.rs:1089-1159` `render_diagnose_churn_skill`

#### `dup-0158` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/cli.rs:403-430` `persona_is_the_system_prompt_task_is_the_prompt`
- `src/driver/cli.rs:433-444` `recurse_false_drops_the_agent_tool_from_allowed_tools`

#### `dup-0159` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/replay.rs, tests/spawn_target_dir_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:376-378` `no_emit`
- `tests/spawn_target_dir_periphery.rs:86-88` `no_emit`

#### `dup-0160` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:380-387` `worker`
- `src/driver/replay.rs:1596-1603` `reviewer`

#### `dup-0161` (near, 2 sites)

Proposed home: `replay::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:605-618` `reclaim_unit_mutation_scratch_never_cross_matches_a_unit_id_that_is_a_string_prefix_of_another`
- `src/driver/replay.rs:624-635` `reclaim_unit_mutation_scratch_is_a_no_op_for_an_empty_unit_id`

#### `dup-0162` (near, 4 sites)

Proposed home: `replay::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/replay.rs:1929-2027` `an_emit_only_approve_gating_persona_hard_errors_on_the_replay_driver`
- `src/driver/replay.rs:2030-2127` `a_concurrent_sibling_approve_does_not_hard_error_a_units_genuine_empty_verdict_reject`
- `src/driver/replay.rs:2130-2227` `a_parked_unanswered_sibling_does_not_suppress_this_units_own_approve_backstop`
- `src/driver/replay.rs:2230-2330` `a_closed_sibling_window_overlapping_this_units_own_approve_still_hard_errors`

#### `dup-0163` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/driver/workflow.rs, src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/driver/workflow.rs:83-85` `new`
- `src/watch.rs:577-579` `new`

#### `dup-0164` (near, 3 sites)

Proposed home: `contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:172-192` `append_assigns_revisions`
- `src/eventstore/contract.rs:296-341` `backward_stream_read_reverses_set`
- `src/eventstore/contract.rs:345-370` `forward_stream_read_honors_nonzero_from`

#### `dup-0165` (near, 2 sites)

Proposed home: `contract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:248-269` `subscription_replays_then_goes_live`
- `src/eventstore/contract.rs:271-292` `stream_subscription_replays_then_goes_live`

#### `dup-0166` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/contract.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:825-832` `read_stream`
- `src/spawn.rs:1492-1499` `read_stream`

#### `dup-0167` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/contract.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/contract.rs:844-846` `subscribe_stream`
- `src/spawn.rs:1514-1516` `subscribe_stream`

#### `dup-0168` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/kurrentdb.rs, src/eventstore/sqlite.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/kurrentdb.rs:121-123` `from_nanos`
- `src/eventstore/sqlite.rs:1474-1476` `from_nanos`

#### `dup-0169` (near, 2 sites)

Proposed home: `kurrentdb::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/kurrentdb.rs:612-617` `a_single_event_reports_the_position_the_server_issued`
- `src/eventstore/kurrentdb.rs:647-657` `a_batch_reports_the_revision_span_the_ack_names`

#### `dup-0170` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:121-124` `with_valid_from`
- `src/spawn.rs:568-571` `with_meta`

#### `dup-0171` (semantic, 2 sites)

Proposed home: `mod::appended - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/eventstore/mod.rs:158-162` `all`
- `src/eventstore/mod.rs:166-168` `from_placements`

#### `dup-0172` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/sidecar.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:536-541` `drop`
- `src/sidecar.rs:219-224` `drop`

#### `dup-0173` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:797-803` `strips_userinfo_and_query_keeps_scheme_host_port`
- `src/eventstore/mod.rs:822-828` `a_credential_smuggled_after_the_path_is_dropped_with_the_path`
- `src/spec.rs:1938-1945` `strip_inline_code_direct_exact_output_pins_a_zero_width_quote_pair`

#### `dup-0174` (exact, 7 sites)

Proposed home: `a new shared module (sites span 4 files: src/eventstore/mod.rs, src/spec.rs, tests/simplification_audit.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:806-811` `strips_a_bare_user_with_no_password`
- `src/eventstore/mod.rs:814-819` `an_already_credential_free_endpoint_is_unchanged`
- `src/eventstore/mod.rs:891-896` `strips_a_userinfo_with_no_password`
- `src/eventstore/mod.rs:930-935` `plain_text_with_no_url_is_untouched`
- `src/spec.rs:1567-1573` `ownership_check_recognizes_owner_inside_a_hyphenated_compound`
- `tests/simplification_audit.rs:4549-4551` `impl_self_type_still_handles_a_generic_self_type_with_a_where_clause`
- `tests/store_secrets_periphery.rs:92-94` `redact_conn_on_the_empty_string_is_empty`

#### `dup-0175` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:836-853` `a_delimiter_inside_the_userinfo_never_leaks_the_credential_head`
- `src/spec.rs:1898-1919` `strip_inline_code_direct_exact_output_pins_the_one_span_per_kind_rule`

#### `dup-0176` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:878-888` `strips_user_and_password_but_keeps_scheme_host_and_query`
- `tests/store_secrets_periphery.rs:78-88` `redact_conn_scrubs_the_whole_userinfo_when_the_authority_has_several_at_signs`

#### `dup-0177` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/mod.rs, tests/store_secrets_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/mod.rs:899-906` `leaves_a_conn_with_no_userinfo_unchanged`
- `tests/store_secrets_periphery.rs:65-72` `redact_conn_leaves_an_at_sign_in_the_path_alone`

#### `dup-0178` (semantic, 2 sites)

Proposed home: `one shared `content_key_index_name` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/eventstore/sqlite.rs:226-236` `content_key_index_name`
- `tests/store_content_identity_periphery.rs:1752-1773` `content_key_index_name`

#### `dup-0179` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, src/metrics.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1103-1109` `factor`
- `src/metrics.rs:362-368` `cost_per_upheld`

#### `dup-0180` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, src/grounder/symbols/events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1780-1785` `direction_sql`
- `src/grounder/symbols/events.rs:713-724` `kind_str`
- `src/grounder/symbols/events.rs:728-737` `lang_str`

#### `dup-0181` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, tests/reset_derived_compaction_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1812-1827` `subject_of`
- `tests/reset_derived_compaction_periphery.rs:2108-2123` `path_subject_of`
- `tests/store_content_identity_periphery.rs:233-249` `path_subject_of`

#### `dup-0182` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, src/ingest.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1830-1833` `split`
- `src/ingest.rs:336-339` `derived_key_parts`
- `tests/published_content_key_split_periphery.rs:79-82` `split`

#### `dup-0183` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1838-1840` `identity`
- `src/eventstore/sqlite.rs:2705-2707` `other_identity`
- `tests/store_content_identity_periphery.rs:264-266` `project_policy`

#### `dup-0184` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: src/eventstore/sqlite.rs, tests/published_content_key_split_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1842-1844` `keyed`
- `src/eventstore/sqlite.rs:2711-2713` `other_keyed`
- `tests/published_content_key_split_periphery.rs:86-88` `keyed`
- `tests/store_content_identity_periphery.rs:274-276` `keyed`

#### `dup-0185` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/eventstore/sqlite.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:1847-1852` `batch`
- `tests/store_content_identity_periphery.rs:279-284` `batch`

#### `dup-0186` (near, 2 sites)

Proposed home: `sqlite::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:2109-2137` `one_files_generations_never_leak_into_another_files_subject`
- `src/eventstore/sqlite.rs:2488-2523` `a_generation_that_is_a_string_prefix_of_a_later_one_is_still_found`

#### `dup-0187` (near, 3 sites)

Proposed home: `sqlite::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/eventstore/sqlite.rs:3618-3655` `measure_derived_duplication_scopes_to_the_stream_prefix`
- `src/eventstore/sqlite.rs:3658-3686` `measure_derived_duplication_on_a_clean_log_reports_no_duplication`
- `src/eventstore/sqlite.rs:3689-3743` `measure_derived_duplication_treats_the_same_key_under_two_covered_types_as_two_distinct_subjects`

#### `dup-0188` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/failure.rs, src/gate.rs, src/ledger.rs, src/watch.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/failure.rs:43-49` `as_str`
- `src/gate.rs:77-83` `as_str`
- `src/ledger.rs:47-59` `as_str`
- `src/watch.rs:192-201` `response`

#### `dup-0189` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: src/failure.rs, src/gate.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/failure.rs:65-67` `reruns`
- `src/failure.rs:74-76` `demotes_on_persistent_failure`
- `src/gate.rs:51-53` `runs_inline`

#### `dup-0190` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:31-37` `parse`
- `src/gate.rs:69-75` `parse`

#### `dup-0191` (near, 3 sites)

Proposed home: `gate::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1124-1158` `exec_runner_exports_cargo_target_dir_only_when_given`
- `src/gate.rs:1161-1188` `exec_runner_forces_cargo_target_dir_onto_build_cache_dir_when_target_dir_is_empty`
- `src/gate.rs:1220-1241` `exec_runner_target_dir_wins_over_build_cache_dir_when_both_are_given`

#### `dup-0192` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1191-1217` `exec_runner_env_vars_reach_the_gate_command_through_the_flock_guard_wrapper`
- `src/gate.rs:1344-1371` `exec_runner_degrades_to_unguarded_when_the_guard_path_cannot_be_opened`

#### `dup-0193` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: src/gate.rs, src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/reap_before_removal_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1247-1255` `wait_until`
- `src/reap.rs:467-475` `wait_until`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:65-73` `wait_until`
- `tests/reap_before_removal_periphery.rs:52-60` `wait_until`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:149-157` `wait_until`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:66-74` `wait_until`

#### `dup-0194` (near, 4 sites)

Proposed home: `gate::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1535-1554` `build_env_resolves_wrapper_cache_dir_and_incremental_off_when_configured`
- `src/gate.rs:1557-1564` `build_env_derives_the_wrapper_specific_cache_dir_var_name`
- `src/gate.rs:1638-1645` `build_env_jobs_cap_reaches_the_build_when_set`
- `src/gate.rs:1648-1660` `build_env_jobs_cap_is_independent_of_the_wrapper`

#### `dup-0195` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1583-1622` `exec_runner_applies_the_build_env_it_is_given`
- `src/gate.rs:1679-1697` `exec_runner_applies_the_jobs_cap_it_is_given`

#### `dup-0196` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1735-1744` `resolve_wrapper_name_auto_probes_known_wrappers_and_finds_one_present`
- `src/gate.rs:1759-1769` `resolve_wrapper_name_named_wrapper_present_on_path_resolves_to_itself`

#### `dup-0197` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1772-1785` `resolve_wrapper_name_named_wrapper_absent_from_path_errors_naming_the_binary`
- `src/gate.rs:1843-1858` `resolve_mutation_layer_on_with_the_binary_absent_errors_naming_the_binary_and_key`

#### `dup-0198` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1788-1797` `resolve_wrapper_name_ignores_a_same_named_non_executable_file_on_path`
- `src/gate.rs:1861-1868` `resolve_mutation_layer_ignores_a_same_named_non_executable_file_on_path`

#### `dup-0199` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1822-1830` `resolve_mutation_layer_off_ignores_a_path_that_actually_has_the_binary`
- `src/gate.rs:1833-1840` `resolve_mutation_layer_on_with_the_binary_present_resolves_true`

#### `dup-0200` (near, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1928-1944` `resolve_build_layer_named_wrapper_with_an_uncreatable_dir_errors_naming_dir_and_key`
- `src/gate.rs:2012-2031` `resolve_build_layer_named_wrapper_with_a_preexisting_unwritable_dir_errors_naming_dir_and_key`

#### `dup-0201` (exact, 2 sites)

Proposed home: `gate::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/gate.rs:1947-1961` `resolve_build_layer_auto_with_an_uncreatable_dir_skips_the_whole_layer`
- `src/gate.rs:2035-2050` `resolve_build_layer_auto_with_a_preexisting_unwritable_dir_skips_the_whole_layer`

#### `dup-0202` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/events.rs:20-43` `concept_events`
- `src/grounder/design/events.rs:51-74` `link_events`

#### `dup-0203` (semantic, 2 sites)

Proposed home: `one shared `project_batches` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/design/events.rs:90-114` `project_batches`
- `src/grounder/symbols/events.rs:88-90` `project_batches`

#### `dup-0204` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/events.rs:200-214` `the_emit_is_deterministic_and_sorts_by_kind_then_id`
- `src/grounder/design/events.rs:320-337` `the_link_emit_is_deterministic_and_sorts_by_rel_then_from_then_to`

#### `dup-0205` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/grounder/design/extract.rs, src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:433-436` `is_markdown`
- `src/spec.rs:541-548` `starts_new_element`
- `tests/simplification_audit.rs:2417-2424` `looks_error_shaping`

#### `dup-0206` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:532-537` `first_heading`
- `src/grounder/design/extract.rs:540-546` `section_headings`

#### `dup-0207` (near, 4 sites)

Proposed home: `extract::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:602-611` `a_load_bearing_decision_doc_becomes_a_single_arch_decision_node`
- `src/grounder/design/extract.rs:614-620` `a_spec_shape_or_loop_discipline_doc_becomes_a_handbook_rule_node`
- `src/grounder/design/extract.rs:623-637` `a_why_comment_in_a_source_file_becomes_a_rationale_node`
- `src/grounder/design/extract.rs:957-966` `a_source_file_is_never_a_usage_doc_and_its_rationale_stays_in_scope`

#### `dup-0208` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/extract.rs:748-761` `a_rationale_explains_the_file_it_annotates`
- `src/grounder/design/extract.rs:764-780` `a_fenced_code_example_path_is_not_mistaken_for_a_specifies_link`

#### `dup-0209` (near, 2 sites)

Proposed home: `model::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/design/model.rs:30-37` `node_kind`
- `src/grounder/design/model.rs:81-89` `rel`

#### `dup-0210` (semantic, 5 sites)

Proposed home: `one error-shaping helper module`

mandatory sweep: error-shaping helper functions - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/mod.rs:207-213` `retired_grounder_error`
- `src/worktree.rs:1552-1606` `revert_on_base_aborts_and_errors_on_a_conflicting_revert`
- `tests/batched_fold_cadence.rs:184-262` `append_and_fold_batch_is_best_effort_on_fold_error_and_a_no_op_on_an_empty_batch`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:352-440` `run_canary_runs_every_item_to_completion_even_when_one_items_spawn_errors`
- `tests/grounder_name_contract.rs:40-171` `public_name_contract_predicate_error_and_resolver_agree`

#### `dup-0211` (near, 2 sites)

Proposed home: `events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/events.rs:276-290` `empty_structural_boundary_event`
- `src/grounder/symbols/events.rs:421-435` `empty_evidence_boundary_event`

#### `dup-0212` (semantic, 3 sites)

Proposed home: `src/grounder/symbols/extract.rs::extract as the ONE function that touches source parsing (already its own module doc's claim, architecture 5.5.3) - this file's own scan_file/tokenize are ad hoc scanners for the identical job and should route through an injected-grammar extractor rather than re-deriving structure by hand`

mandatory sweep: bespoke source-text lexer/scanner functions duplicating the canonical tree-sitter extractor - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/extract.rs:34-178` `extract`
- `tests/simplification_audit.rs:187-461` `scan_file`
- `tests/simplification_audit.rs:1827-1933` `tokenize`

#### `dup-0213` (near, 7 sites)

Proposed home: `extract::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/extract.rs:860-928` `test_annotated_definitions_and_everything_nested_inside_them_are_marked_is_test`
- `src/grounder/symbols/extract.rs:931-969` `cfg_predicates_naming_test_are_recognized_and_similarly_spelled_tokens_are_not`
- `src/grounder/symbols/extract.rs:972-1007` `negated_and_cfg_attr_predicates_naming_test_do_not_mark_the_item_test`
- `src/grounder/symbols/extract.rs:1010-1045` `a_not_wrapping_a_non_test_atom_never_marks_the_item_test`
- `src/grounder/symbols/extract.rs:1048-1083` `compound_predicates_with_nested_negation_or_a_non_test_disjunct_do_not_mark_the_item_test`
- `src/grounder/symbols/extract.rs:1086-1126` `a_trailing_same_line_comment_on_a_cfg_test_attribute_does_not_sever_the_scan`
- `src/grounder/symbols/extract.rs:1129-1182` `an_inner_cfg_test_attribute_marks_its_enclosing_module_and_the_module_marks_its_children`

#### `dup-0214` (near, 2 sites)

Proposed home: `extract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/extract.rs:1273-1319` `extent_spans_a_destructuring_or_default_brace_signature_to_the_full_body`
- `src/grounder/symbols/extract.rs:1322-1384` `extent_generalizes_across_grammars_python_nested_def_and_js_brace_string`

#### `dup-0215` (semantic, 2 sites)

Proposed home: `one shared `changed_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/grounder/symbols/grounder.rs:75-98` `changed_files`
- `src/worktree.rs:470-473` `changed_files`

#### `dup-0216` (near, 3 sites)

Proposed home: `grounder::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/grounder.rs:807-847` `a_concurrent_reindex_from_a_second_grounder_is_not_clobbered`
- `src/grounder/symbols/grounder.rs:850-873` `reindex_replaces_only_a_changed_files_symbols`
- `src/grounder/symbols/grounder.rs:876-903` `reindex_over_a_deleted_file_stops_grounding_it`

#### `dup-0217` (near, 2 sites)

Proposed home: `mod::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/mod.rs:319-329` `a_file_added_to_the_tree_since_the_index_was_built_is_flagged`
- `src/grounder/symbols/mod.rs:332-347` `a_file_removed_from_the_tree_since_the_index_was_built_is_flagged`

#### `dup-0218` (exact, 2 sites)

Proposed home: `model::symbol_index`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/model.rs:169-171` `insert_file`
- `src/grounder/symbols/model.rs:189-191` `set_hash`

#### `dup-0219` (near, 2 sites)

Proposed home: `store::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/grounder/symbols/store.rs:23-28` `index_path`
- `src/grounder/symbols/store.rs:33-38` `lock_path`

#### `dup-0220` (semantic, 2 sites)

Proposed home: `ledger::attention_entry - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/ledger.rs:180-190` `unit_scoped`
- `src/ledger.rs:193-199` `run_scoped`

#### `dup-0221` (near, 2 sites)

Proposed home: `ledger::run_state`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ledger.rs:569-574` `is_terminal`
- `src/ledger.rs:577-582` `is_integrated`

#### `dup-0222` (near, 6 sites)

Proposed home: `a new shared module (sites span 2 files: src/ledger.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/ledger.rs:814-821` `short_run_id_truncates_to_twelve_chars_and_passes_shorter_ids_through`
- `src/ledger.rs:824-839` `spec_stem_extracts_and_sanitizes_the_file_stem`
- `tests/simplification_audit.rs:4535-4546` `impl_self_type_strips_a_trailing_where_clause_on_a_non_generic_self_type`
- `tests/simplification_audit.rs:4554-4560` `strip_trailing_where_clause_is_a_word_boundary_match_not_a_substring_match`
- `tests/simplification_audit.rs:4602-4606` `pluralize_functions_uses_singular_only_at_exactly_one`
- `tests/simplification_audit.rs:4896-4906` `ident_kind_marker_classifies_by_casing`

#### `dup-0223` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:486-499` `marker_filename_hex_escapes_every_byte_outside_alphanumeric_and_hyphen`
- `src/liveness.rs:502-521` `marker_filename_hex_escapes_dots_so_no_encoded_result_can_ever_be_a_path_traversal_component`

#### `dup-0224` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/liveness.rs, src/spec.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:524-539` `marker_filename_is_none_only_for_a_truly_empty_input_so_a_join_can_never_be_a_no_op`
- `src/spec.rs:2187-2189` `heading_level_rejects_more_than_six_hashes`

#### `dup-0225` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/liveness.rs, src/main.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:567-581` `marker_filename_is_injective_so_two_ids_that_collided_under_a_prior_placeholder_scheme_no_longer_do`
- `src/main.rs:18678-18692` `normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host`

#### `dup-0226` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/liveness.rs, src/worktree.rs, tests/run_scoping_survives_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:734-736` `read`
- `src/worktree.rs:1960-1962` `read_stream`
- `tests/run_scoping_survives_periphery.rs:133-135` `read`

#### `dup-0227` (near, 2 sites)

Proposed home: `liveness::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/liveness.rs:982-995` `any_marker_fresh_finds_a_fresh_marker_nested_under_a_run_id_directory`
- `src/liveness.rs:998-1012` `any_marker_fresh_is_false_once_every_marker_is_older_than_max_age`

#### `dup-0228` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:539-542` `config_rigger_dir`
- `src/main.rs:907-910` `project_identity`
- `src/main.rs:12465-12468` `git_repo`

#### `dup-0229` (semantic, 2 sites)

Proposed home: `one shared `project_identity` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/main.rs:907-910` `project_identity`
- `tests/reset_derived_compaction_periphery.rs:616-638` `project_identity`

#### `dup-0230` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:1635-1637` `usage`
- `src/main.rs:11040-11047` `print_scaffold_pointer`

#### `dup-0231` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:4124-4183` `cmd_graph_communities`
- `src/main.rs:4203-4262` `cmd_graph_concepts`

#### `dup-0232` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:10206-10225` `dir_size_bytes`
- `tests/reset_build_cache_periphery.rs:91-108` `dir_bytes`

#### `dup-0233` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:11235-11237` `shim_dir`
- `src/main.rs:11335-11337` `docs_overlay_path`

#### `dup-0234` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/main.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:12476-12486` `git_repo_at`
- `tests/heartbeat_write_read_agree_periphery.rs:68-78` `git_toplevel`
- `tests/reset_derived_live_writer_guard_periphery.rs:57-68` `git_toplevel`

#### `dup-0235` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:14042-14048` `run_started_at`
- `src/main.rs:14049-14055` `decision`

#### `dup-0236` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: src/main.rs, tests/graph_click_to_seed_repoint.rs, tests/graph_seeds_repoint_denoise.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:14112-14116` `ev`
- `tests/graph_click_to_seed_repoint.rs:26-30` `ev`
- `tests/graph_seeds_repoint_denoise.rs:23-27` `ev`

#### `dup-0237` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:14596-14635` `per_operation_skills_reference_only_real_subcommands`
- `src/main.rs:14645-14673` `watching_discipline_skills_reference_only_real_subcommands`

#### `dup-0238` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15205-15249` `git_is_ancestor_decides_commit_order_in_a_real_repo`
- `src/main.rs:15252-15298` `git_commit_distance_counts_commits_ahead_in_a_real_repo`

#### `dup-0239` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15318-15323` `behind_the_tree_message_is_silent_when_versions_already_match`
- `src/main.rs:15326-15336` `behind_the_tree_message_is_silent_when_either_side_is_unversioned`
- `src/main.rs:15339-15348` `behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance`

#### `dup-0240` (exact, 24 sites)

Proposed home: `a new shared module (sites span 23 files: src/main.rs, tests/adaptive_labels_periphery.rs, tests/code_lens_overview_collapse_viz.rs, tests/concepts_lens_view_periphery.rs, tests/dash_calls_render_viz.rs, tests/dash_decisions_progressive_disclosure.rs, tests/dash_graph_exploration_viz.rs, tests/dash_kg_graph_route.rs, tests/dash_release_ready.rs, tests/files_lens_directory_hulls_viz.rs, tests/gitsemver_derivation.rs, tests/gitsemver_worktree_periphery.rs, tests/graph_collision_body_and_tiebreak.rs, tests/graph_density_spread_floor_and_centring.rs, tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_lens_overlay_served_page.rs, tests/subject_view_memory_rail_client.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15365-15371` `gitsemver_available`
- `src/main.rs:21482-21488` `npm_available`
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
- `tests/proof_row_renders_on_the_card.rs:33-39` `node_available`
- `tests/readable_graph_adaptive_labels.rs:59-65` `node_available`
- `tests/readable_graph_density_scaled_spacing.rs:50-56` `node_available`
- `tests/readable_graph_layout_separation.rs:44-50` `node_available`
- `tests/subject_lens_overlay_client_arms.rs:43-49` `node_available`
- `tests/subject_lens_overlay_served_page.rs:48-54` `node_available`
- `tests/subject_view_memory_rail_client.rs:34-40` `node_available`
- `tests/validate_behind_the_tree_periphery.rs:139-145` `gitsemver_available`

#### `dup-0241` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: src/main.rs, tests/build_watch_paths.rs, tests/gitsemver_derivation.rs, tests/gitsemver_worktree_periphery.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15375-15390` `behind_the_tree_git`
- `tests/build_watch_paths.rs:42-53` `git`
- `tests/gitsemver_derivation.rs:43-54` `git`
- `tests/gitsemver_worktree_periphery.rs:56-67` `git`
- `tests/validate_behind_the_tree_periphery.rs:94-109` `git`

#### `dup-0242` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/main.rs, tests/build_watch_paths.rs, tests/gitsemver_worktree_periphery.rs, tests/validate_behind_the_tree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15392-15404` `behind_the_tree_git_output`
- `tests/build_watch_paths.rs:59-74` `git_output`
- `tests/gitsemver_worktree_periphery.rs:75-90` `git_output`
- `tests/validate_behind_the_tree_periphery.rs:113-134` `git_output`

#### `dup-0243` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/reset_build_cache_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15514-15517` `write_file`
- `tests/reset_build_cache_periphery.rs:69-72` `write_file`

#### `dup-0244` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:15580-15597` `refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them`
- `src/main.rs:15600-15618` `refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds`
- `src/main.rs:15621-15661` `refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor`

#### `dup-0245` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16781-16789` `footprint_advisories_is_silent_below_the_threshold`
- `src/main.rs:16825-16833` `footprint_advisories_is_silent_on_an_empty_category`

#### `dup-0246` (near, 3 sites)

Proposed home: `main::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:16836-16859` `owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel`
- `src/main.rs:17105-17117` `find_store_dir_from_walks_up_from_a_subdirectory`
- `src/main.rs:17349-17378` `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above`

#### `dup-0247` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, src/reap.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17065-17072` `leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent`
- `src/reap.rs:539-546` `processes_rooted_under_is_a_graceful_no_op_when_the_base_is_absent`

#### `dup-0248` (exact, 3 sites)

Proposed home: `main::restore`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17517-17519` `drop`
- `src/main.rs:22745-22747` `drop`
- `src/main.rs:22772-22774` `drop`

#### `dup-0249` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:17778-17816` `a_recorded_result_lets_the_replay_driver_advance_past_the_spawn`
- `src/main.rs:17819-17851` `a_recorded_error_result_replays_as_a_failure_not_a_fake_success`

#### `dup-0250` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18126-18131` `parse_run_args_rejects_unknown_flags_and_values`
- `src/main.rs:23891-23895` `parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag`

#### `dup-0251` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:18811-18876` `migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned`
- `src/main.rs:18962-19057` `migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename`

#### `dup-0252` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:19803-19882` `init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently`
- `src/main.rs:19891-19931` `init_project_gitignores_the_store_conn_secret_file_idempotently`

#### `dup-0253` (near, 4 sites)

Proposed home: `main::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:20336-20349` `import_agents_validates_and_rejects_a_malformed_agent`
- `src/main.rs:20358-20380` `import_agents_rejects_an_id_colliding_with_an_existing_agent`
- `src/main.rs:20386-20406` `import_agents_rejects_a_duplicate_id_within_one_import`
- `src/main.rs:20412-20433` `import_agents_rejects_an_agent_with_a_blank_id`

#### `dup-0254` (exact, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:20757-20768` `the_step_schema_admits_the_attention_array`
- `src/main.rs:22820-22823` `no_runs_message_points_at_rigger_run`

#### `dup-0255` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: src/main.rs, tests/meta_phases_declaration_periphery.rs, tests/phase_of_role_mapping_periphery.rs, tests/review_tier_roster_periphery.rs, tests/step_attention_periphery.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:20774-20796` `js_function_body`
- `tests/meta_phases_declaration_periphery.rs:65-87` `js_declaration`
- `tests/phase_of_role_mapping_periphery.rs:50-72` `js_declaration`
- `tests/review_tier_roster_periphery.rs:18-40` `js_declaration`
- `tests/step_attention_periphery.rs:655-677` `js_declaration`
- `tests/worker_persona_label_periphery.rs:21-43` `js_declaration`

#### `dup-0256` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:21570-21587` `format_canary_stats_reports_findings_raised_by_tier`
- `src/main.rs:21592-21602` `format_canary_stats_reports_a_zero_findings_count_honestly`

#### `dup-0257` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:21608-21614` `format_canary_stats_omits_the_findings_volume_section_when_empty`
- `src/main.rs:21836-21842` `format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one`

#### `dup-0258` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:21752-21771` `format_canary_stats_reports_control_items_and_false_positives`
- `src/main.rs:21779-21794` `format_canary_stats_reports_zero_false_positives_honestly`

#### `dup-0259` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22051-22096` `stats_discloses_when_no_verdict_was_recorded_on_this_driver`
- `src/main.rs:22107-22180` `stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed`

#### `dup-0260` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:22842-22854` `stats_lines_absent_db_returns_none_and_creates_no_file`
- `src/main.rs:23052-23064` `result_of_at_absent_db_reads_as_unreported_and_creates_no_file`

#### `dup-0261` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23070-23090` `result_of_at_unrecorded_spawn_reads_as_unreported`
- `src/main.rs:23128-23157` `result_of_at_is_namespace_scoped`

#### `dup-0262` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/main.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23187-23200` `pgid_of`
- `tests/cli.rs:23419-23432` `proc_pgid_of`

#### `dup-0263` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:23661-23682` `reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode`
- `src/main.rs:23689-23706` `reset_modes_parses_build_cache_alone_and_composed_and_rejects_duplicates`

#### `dup-0264` (near, 2 sites)

Proposed home: `main::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/main.rs:24216-24291` `implementer_persona_pins_the_seeded_mutation_step_contract`
- `src/main.rs:24308-24345` `implementer_persona_pins_the_seeded_mutation_scratch_root_registration_contract`

#### `dup-0265` (exact, 2 sites)

Proposed home: `mcpserver::tool_error`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:29-34` `internal`
- `src/mcpserver.rs:38-43` `invalid_params`

#### `dup-0266` (semantic, 4 sites)

Proposed home: `mcpserver::tool_error - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/mcpserver.rs:29-34` `internal`
- `src/mcpserver.rs:38-43` `invalid_params`
- `src/mcpserver.rs:47-49` `from`
- `src/mcpserver.rs:53-55` `from`

#### `dup-0267` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1127-1145` `emit_tool_carries_meta_actor`
- `src/mcpserver.rs:1148-1170` `emit_tool_sets_valid_from_from_nanos`

#### `dup-0268` (near, 2 sites)

Proposed home: `mcpserver::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1184-1222` `peers_tool_scopes_to_the_files_arg`
- `src/mcpserver.rs:1225-1270` `peers_tool_surfaces_findings_scoped_to_the_files_arg`

#### `dup-0269` (near, 4 sites)

Proposed home: `mcpserver::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/mcpserver.rs:1322-1341` `rigger_result_for_an_unknown_id_is_an_error`
- `src/mcpserver.rs:1344-1366` `malformed_json_gets_a_parse_error`
- `src/mcpserver.rs:1369-1387` `request_missing_method_gets_an_invalid_request_error`
- `src/mcpserver.rs:1390-1408` `tools_call_missing_name_gets_an_invalid_params_error`

#### `dup-0270` (exact, 7 sites)

Proposed home: `metrics::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:341-343` `survival`
- `src/metrics.rs:435-437` `lens_overlap_rate`
- `src/metrics.rs:449-451` `first_pass_yield`
- `src/metrics.rs:455-457` `escalation_rate`
- `src/metrics.rs:1087-1089` `rate`
- `src/metrics.rs:1152-1154` `adjudicator_accuracy`
- `src/metrics.rs:1158-1160` `stability_rate`

#### `dup-0271` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1009-1011` `gate_verdict`
- `src/run.rs:100-102` `from_event`

#### `dup-0272` (semantic, 2 sites)

Proposed home: `one shared `gate_verdict` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/metrics.rs:1009-1011` `gate_verdict`
- `tests/dash_run_tree_spine.rs:80-86` `gate_verdict`

#### `dup-0273` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/spawn.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1323-1325` `changed`
- `src/spawn.rs:574-576` `is_error`

#### `dup-0274` (exact, 3 sites)

Proposed home: `metrics::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1404-1409` `started`
- `src/metrics.rs:1411-1416` `status`
- `src/metrics.rs:1443-1448` `artifact_verdict`

#### `dup-0275` (near, 6 sites)

Proposed home: `a new shared module (sites span 2 files: src/metrics.rs, src/run.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1418-1423` `failed`
- `src/metrics.rs:1425-1430` `integrated`
- `src/metrics.rs:1432-1434` `escalated`
- `src/run.rs:507-509` `decision`
- `src/run.rs:510-512` `finding`
- `src/run.rs:513-515` `lesson`

#### `dup-0276` (near, 5 sites)

Proposed home: `metrics::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1738-1751` `counts_review_rejects_on_both_per_unit_and_fan_out_paths`
- `src/metrics.rs:1806-1818` `fan_out_reject_then_approve_counts_one_each`
- `src/metrics.rs:1866-1879` `duplicate_unit_started_counts_the_unit_once`
- `src/metrics.rs:1882-1899` `interleaved_units_keep_per_id_review_state`
- `src/metrics.rs:1902-1913` `escalation_is_counted_once_per_unit`

#### `dup-0277` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:1970-1981` `finding`
- `src/metrics.rs:1986-1996` `courier_finding`

#### `dup-0278` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2134-2154` `spawn_timing_never_pairs_a_cross_run_id_collision`
- `src/metrics.rs:2251-2266` `spawn_timing_excludes_a_same_batch_zero_duration_pair_as_suspect`

#### `dup-0279` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2309-2332` `finding_survival_is_upheld_over_raised_per_actor`
- `src/metrics.rs:2404-2432` `cost_per_upheld_finding_is_spawns_over_upheld_per_tier`

#### `dup-0280` (near, 2 sites)

Proposed home: `metrics::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/metrics.rs:2853-2872` `project_canary_counts_correctly_rejected_items_with_empty_attribution`
- `src/metrics.rs:2881-2899` `project_canary_counts_controls_and_false_positives`

#### `dup-0281` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/progress.rs, tests/reset_derived_compaction.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/progress.rs:197-200` `event_unit_id`
- `tests/reset_derived_compaction.rs:161-164` `replay_key`

#### `dup-0282` (near, 6 sites)

Proposed home: `a new shared module (sites span 5 files: src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/reap_before_removal_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:441-447` `sleeper_in`
- `src/reap.rs:451-458` `sigterm_ignorer_in`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:54-61` `sigterm_ignorer_in`
- `tests/reap_before_removal_periphery.rs:41-48` `sigterm_ignorer_in`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:138-145` `sigterm_ignorer_in`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:55-62` `sigterm_ignorer_in`

#### `dup-0283` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: src/reap.rs, tests/mutation_scratch_reap_base_guard_periphery.rs, tests/no_os_kill_test_helper_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:479-482` `cleanup`
- `tests/mutation_scratch_reap_base_guard_periphery.rs:77-80` `cleanup`
- `tests/no_os_kill_test_helper_periphery.rs:84-87` `cleanup`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:161-164` `cleanup`

#### `dup-0284` (exact, 2 sites)

Proposed home: `reap::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:585-590` `is_reapable_base_refuses_a_dir_that_is_not_under_the_given_authorized_root`
- `src/reap.rs:593-596` `is_reapable_base_refuses_the_authorized_root_itself`

#### `dup-0285` (near, 2 sites)

Proposed home: `reap::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/reap.rs:863-889` `signal_if_unchanged_skips_a_starttime_mismatch`
- `src/reap.rs:892-919` `signal_if_unchanged_skips_when_cwd_is_outside_the_given_base`

#### `dup-0286` (near, 15 sites)

Proposed home: `a new shared module (sites span 11 files: src/registry.rs, tests/reset_build_cache_periphery.rs, tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/store_flag_precedence.rs, tests/store_precedence.rs, tests/store_resolution_cli.rs, tests/store_secrets.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:133-135` `instances_dir`
- `tests/reset_build_cache_periphery.rs:45-47` `event_log`
- `tests/reset_build_cache_periphery.rs:61-63` `shared_cache_dir`
- `tests/reset_build_cache_periphery.rs:65-67` `guard_path`
- `tests/reset_derived_compaction.rs:75-77` `event_log`
- `tests/reset_derived_compaction_periphery.rs:610-612` `event_log`
- `tests/reset_derived_compaction_periphery.rs:1364-1366` `graph_db`
- `tests/reset_menu.rs:70-72` `event_log`
- `tests/reset_menu.rs:74-76` `graph_db`
- `tests/reset_menu_identity_migration_periphery.rs:65-67` `event_log`
- `tests/store_flag_precedence.rs:94-96` `local_event_log`
- `tests/store_precedence.rs:59-61` `local_event_log`
- `tests/store_resolution_cli.rs:66-68` `local_event_log`
- `tests/store_secrets.rs:62-64` `local_event_log`
- `tests/validate_advisories.rs:81-83` `event_log`

#### `dup-0287` (near, 3 sites)

Proposed home: `registry::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:299-313` `write_then_read_round_trips_a_live_entry`
- `src/registry.rs:411-423` `read_live_no_prune_still_returns_a_fresh_entry`
- `src/registry.rs:426-447` `read_all_returns_a_stale_entry_read_live_would_have_pruned_and_never_deletes_it`

#### `dup-0288` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:335-346` `two_projects_get_distinct_entries`
- `src/registry.rs:450-462` `read_all_returns_every_registered_root_regardless_of_freshness`

#### `dup-0289` (near, 2 sites)

Proposed home: `registry::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/registry.rs:349-362` `a_reader_prunes_a_stale_heartbeat`
- `src/registry.rs:383-408` `read_live_no_prune_filters_a_stale_heartbeat_but_never_deletes_its_file`

#### `dup-0290` (near, 2 sites)

Proposed home: `run::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/run.rs:645-703` `the_resolved_base_is_persisted_on_the_run_start_and_survives_adopt`
- `src/run.rs:706-769` `the_spec_path_is_persisted_on_the_run_start_and_survives_adopt`

#### `dup-0291` (near, 2 sites)

Proposed home: `run::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/run.rs:795-812` `ensure_started_adopts_the_same_criteria_run_without_re_minting`
- `src/run.rs:870-888` `ensure_started_mints_a_new_run_when_the_criteria_change`

#### `dup-0292` (exact, 3 sites)

Proposed home: `sidecar::sidecar`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:137-147` `decisions_for`
- `src/sidecar.rs:168-178` `findings_for`
- `src/sidecar.rs:196-206` `lessons_for`

#### `dup-0293` (exact, 2 sites)

Proposed home: `sidecar::sidecar`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:155-161` `findings`
- `src/sidecar.rs:184-190` `lessons`

#### `dup-0294` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/sidecar.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:209-211` `len`
- `tests/store_content_identity_periphery.rs:87-89` `batch_calls`

#### `dup-0295` (near, 3 sites)

Proposed home: `sidecar::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/sidecar.rs:268-308` `decisions_for_scopes_to_the_blast_radius`
- `src/sidecar.rs:311-359` `findings_for_scopes_to_the_blast_radius`
- `src/sidecar.rs:362-413` `lessons_for_scopes_to_the_blast_radius`

#### `dup-0296` (exact, 4 sites)

Proposed home: `spawn::spawn_request`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:338-341` `with_system_prompt`
- `src/spawn.rs:344-347` `with_model`
- `src/spawn.rs:356-359` `with_dir`
- `src/spawn.rs:368-371` `with_title`

#### `dup-0297` (exact, 3 sites)

Proposed home: `spawn::spawn_request`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:350-353` `with_tools`
- `src/spawn.rs:362-365` `with_blast_radius`
- `src/spawn.rs:374-377` `with_reviews`

#### `dup-0298` (exact, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:381-383` `to_event`
- `src/spawn.rs:663-665` `to_event`

#### `dup-0299` (exact, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:386-388` `from_event`
- `src/spawn.rs:668-670` `from_event`

#### `dup-0300` (near, 2 sites)

Proposed home: `spawn::spawn_result`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:527-534` `ok`
- `src/spawn.rs:539-546` `failed`

#### `dup-0301` (semantic, 3 sites)

Proposed home: `spawn::spawn_result - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/spawn.rs:527-534` `ok`
- `src/spawn.rs:539-546` `failed`
- `src/spawn.rs:554-565` `liveness_fault`

#### `dup-0302` (semantic, 2 sites)

Proposed home: `one shared `liveness_fault` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `src/spawn.rs:554-565` `liveness_fault`
- `tests/dash_run_tree_spine.rs:90-94` `liveness_fault`

#### `dup-0303` (exact, 2 sites)

Proposed home: `spawn::spawn_result`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:587-593` `liveness_class`
- `src/spawn.rs:600-606` `resolved_model`

#### `dup-0304` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1417-1451` `record_result_if_absent_is_a_noop_that_never_clobbers_an_existing_result`
- `src/spawn.rs:1566-1603` `record_result_if_absent_honors_a_self_report_that_won_the_race`

#### `dup-0305` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1735-1768` `step_wave_is_the_full_pending_frontier_never_answered_spawns`
- `src/spawn.rs:2022-2054` `step_rerun_reprints_unanswered_spawns_so_a_killed_step_orphans_nothing`

#### `dup-0306` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1797-1824` `spawn_request_carries_a_title_via_builder_and_omits_it_when_empty`
- `src/spawn.rs:1827-1857` `wave_item_copies_the_request_title_so_the_thin_driver_renders_the_work`

#### `dup-0307` (near, 2 sites)

Proposed home: `spawn::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spawn.rs:1921-1961` `spawn_request_carries_a_reviews_roster_via_builder_and_omits_it_when_empty`
- `src/spawn.rs:1964-1999` `wave_item_copies_the_request_reviews_roster`

#### `dup-0308` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:652-654` `find_word`
- `src/spec.rs:662-664` `find_word_across_hyphen`

#### `dup-0309` (exact, 3 sites)

Proposed home: `spec::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:911-925` `extract_criteria_joins_a_three_line_wrap_including_the_owns_sentence_on_line_three`
- `src/spec.rs:931-943` `extract_criteria_stops_a_wrap_at_the_next_checkbox_item_with_no_blank_line_between`
- `src/spec.rs:981-994` `extract_criteria_includes_a_nested_sub_bullet_as_part_of_the_criterion_text`

#### `dup-0310` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:949-959` `extract_criteria_stops_a_wrap_at_a_blank_line_and_excludes_the_prose_after_it`
- `src/spec.rs:964-974` `extract_criteria_stops_a_wrap_at_a_following_heading`

#### `dup-0311` (exact, 17 sites)

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

#### `dup-0312` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1041-1050` `a_single_coordinator_does_not_flag_multi_behavior`
- `src/spec.rs:1181-1194` `spec_shape_advisories_ignores_coordinators_added_by_continuation_lines`

#### `dup-0313` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1197-1206` `path_tokens_extracts_relative_file_paths_and_trims_markdown`
- `src/spec.rs:1222-1228` `path_tokens_dedupes_and_preserves_first_seen_order`

#### `dup-0314` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1209-1219` `path_tokens_ignores_prose_flags_versions_types_and_urls`
- `src/spec.rs:1237-1248` `path_tokens_requires_an_alphabetic_extension_and_a_separator`

#### `dup-0315` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1296-1307` `ownership_check_accepts_the_word_owner`
- `src/spec.rs:1316-1332` `ownership_check_finds_an_owns_sentence_on_a_wrapped_continuation_line`

#### `dup-0316` (near, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1351-1375` `ownership_check_finds_an_owns_sentence_on_an_unindented_continuation_line`
- `src/spec.rs:1384-1406` `ownership_check_does_not_reattach_prose_after_a_blank_line_to_the_prior_criterion`

#### `dup-0317` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1414-1430` `ownership_check_does_not_let_a_dropped_word_boundary_fake_an_owns_sentence`
- `src/spec.rs:1442-1458` `ownership_check_does_not_let_a_dropped_word_boundary_weld_own_and_er_into_owner`

#### `dup-0318` (near, 4 sites)

Proposed home: `spec::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1484-1497` `ownership_check_flags_ownerless_and_not_owned_as_denials`
- `src/spec.rs:1507-1526` `ownership_check_does_not_match_owns_or_owner_inside_an_unrelated_word`
- `src/spec.rs:1537-1559` `ownership_check_lets_an_affirmative_owns_win_over_an_unrelated_denial_elsewhere`
- `src/spec.rs:2117-2127` `starts_new_element_recognizes_every_prefix_kind_independently`

#### `dup-0319` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1733-1742` `disposition_check_does_not_exempt_unsatisfied_either_or`
- `src/spec.rs:1781-1790` `disposition_check_still_flags_a_bare_either_or_with_no_satisfied_word`

#### `dup-0320` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1767-1776` `disposition_check_finds_a_genuine_hedge_after_an_earlier_non_disjunctive_either`
- `src/spec.rs:1853-1864` `disposition_check_still_fires_outside_a_balanced_quote_pair`

#### `dup-0321` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: src/spec.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:1835-1847` `disposition_check_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `tests/simplification_audit.rs:4317-4321` `a_trait_method_signature_without_a_body_is_not_recorded`

#### `dup-0322` (exact, 2 sites)

Proposed home: `spec::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/spec.rs:2092-2102` `disposition_check_attributes_a_hit_inside_a_criterion`
- `src/spec.rs:2145-2155` `hygiene_check_attributes_a_hit_inside_a_criterion`

#### `dup-0323` (near, 4 sites)

Proposed home: `watch::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:673-679` `a_clean_store_detects_no_anomalies`
- `src/watch.rs:737-751` `two_failures_below_threshold_is_not_reported`
- `src/watch.rs:754-775` `a_cause_change_resets_the_streak_so_three_failures_split_across_two_causes_do_not_alert`
- `src/watch.rs:825-831` `a_spawn_answered_twice_is_below_the_frontier_stall_threshold`

#### `dup-0324` (near, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:708-734` `a_unit_at_reject_recurrence_three_same_cause_is_reported`
- `src/watch.rs:797-822` `a_spawn_answered_three_times_is_reported_as_a_frontier_stall`

#### `dup-0325` (near, 3 sites)

Proposed home: `watch::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:881-904` `a_fresh_heartbeat_suppresses_the_dead_driver_alert_even_with_a_quiet_store`
- `src/watch.rs:907-935` `a_heartbeat_ten_minutes_stale_does_not_cross_the_thirty_minute_bound`
- `src/watch.rs:938-969` `a_heartbeat_exactly_thirty_minutes_stale_does_not_yet_cross_the_bound`

#### `dup-0326` (near, 3 sites)

Proposed home: `watch::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1028-1051` `a_dash_marker_naming_a_dead_pid_is_reported`
- `src/watch.rs:1061-1089` `a_dead_dash_url_with_no_marker_is_reported_without_inventing_a_pid`
- `src/watch.rs:1173-1201` `dash_attempted_this_run_overrides_a_breadcrumb_that_looks_like_it_predates_the_run`

#### `dup-0327` (exact, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1092-1106` `no_dash_ever_recorded_is_not_an_anomaly`
- `src/watch.rs:1109-1123` `a_serving_dash_is_not_an_anomaly`

#### `dup-0328` (near, 2 sites)

Proposed home: `watch::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/watch.rs:1452-1464` `dedup_suppresses_a_persisting_anomaly_at_the_same_magnitude`
- `src/watch.rs:1483-1496` `dedup_re_alerts_a_cleared_and_later_recurring_anomaly`

#### `dup-0329` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:655-666` `branch_exists`
- `src/worktree.rs:675-686` `ref_resolves`

#### `dup-0330` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:742-745` `scratch_root_from_env`
- `src/worktree.rs:749-752` `scratch_root_path_from_env`

#### `dup-0331` (exact, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1333-1340` `head_sha_of`
- `src/worktree.rs:1355-1362` `tree_sha_of`

#### `dup-0332` (near, 3 sites)

Proposed home: `worktree::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1424-1444` `integrate_lands_work_in_the_repo`
- `src/worktree.rs:1653-1686` `commit_cleans_the_tree_so_a_gate_sees_the_committed_artifact`
- `src/worktree.rs:1731-1754` `integrate_lands_a_pre_committed_artifact_unchanged`

#### `dup-0333` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1757-1784` `changed_files_reports_only_the_rename_destination`
- `src/worktree.rs:3406-3418` `changed_files_unquotes_paths_with_spaces`

#### `dup-0334` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:1875-1912` `sweep_terminal_removes_merged_worktrees_and_keeps_inflight_ones`
- `src/worktree.rs:2385-2432` `sweep_terminal_reclaims_a_crash_left_terminal_units_per_unit_build_cache`

#### `dup-0335` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2178-2182` `requested_and_answered`
- `src/worktree.rs:2186-2190` `requested_and_hung`

#### `dup-0336` (near, 4 sites)

Proposed home: `worktree::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2193-2225` `sweep_terminal_spares_a_merged_branch_whose_units_latest_spawn_is_still_in_flight`
- `src/worktree.rs:2228-2253` `sweep_terminal_reclaims_a_merged_branch_once_its_latest_spawn_has_a_real_result`
- `src/worktree.rs:2256-2279` `sweep_terminal_reclaims_a_merged_branch_whose_latest_spawn_is_hung`
- `src/worktree.rs:2282-2304` `sweep_terminal_reclaims_a_merged_branch_with_no_spawn_recorded_at_all_unchanged`

#### `dup-0337` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2307-2346` `sweep_terminal_prints_evidence_for_a_kept_decision_but_not_for_a_removed_no_spawn_one`
- `src/worktree.rs:2349-2382` `sweep_terminal_prints_evidence_for_a_removed_terminal_spawn_decision`

#### `dup-0338` (near, 3 sites)

Proposed home: `worktree::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2504-2545` `worktree_remove_reclaims_the_sibling_per_unit_cache`
- `src/worktree.rs:2548-2583` `worktree_remove_also_reclaims_the_store_fence_sibling`
- `src/worktree.rs:2603-2632` `worktree_remove_also_reclaims_a_review_worktrees_store_fence_sibling`

#### `dup-0339` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2586-2600` `review_fence_sibling_maps_a_review_worktree_to_its_fence_sibling_and_ignores_the_rest`
- `src/worktree.rs:2914-2927` `unit_cache_sibling_maps_a_unit_worktree_to_its_cache_and_ignores_the_rest`

#### `dup-0340` (near, 2 sites)

Proposed home: `worktree::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:2643-2704` `reclaim_worktree_on_branch_deregisters_the_lingering_worktree_reclaims_its_cache_and_frees_the_branch`
- `src/worktree.rs:2861-2911` `reclaim_worktree_on_branch_prunes_a_stale_registration_whose_dir_was_deleted_and_frees_the_branch`

#### `dup-0341` (near, 3 sites)

Proposed home: `worktree::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `src/worktree.rs:3507-3583` `create_self_heals_a_corrupt_worktree_admin_entry_and_spares_healthy_ones`
- `src/worktree.rs:3586-3657` `create_heals_a_zero_length_gitdir_marker_the_commondir_case_leaves_untested`
- `src/worktree.rs:3660-3723` `create_heals_a_fully_missing_marker_not_just_a_truncated_one`

#### `dup-0342` (exact, 19 sites)

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

#### `dup-0343` (semantic, 19 sites)

Proposed home: `one shared `node_available` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 19 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

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
- `tests/proof_row_renders_on_the_card.rs:33-39` `node_available`
- `tests/readable_graph_adaptive_labels.rs:59-65` `node_available`
- `tests/readable_graph_density_scaled_spacing.rs:50-56` `node_available`
- `tests/readable_graph_layout_separation.rs:44-50` `node_available`
- `tests/subject_lens_overlay_client_arms.rs:43-49` `node_available`
- `tests/subject_lens_overlay_served_page.rs:48-54` `node_available`
- `tests/subject_view_memory_rail_client.rs:34-40` `node_available`

#### `dup-0344` (exact, 3 sites)

Proposed home: `adaptive_labels_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adaptive_labels_periphery.rs:467-479` `the_live_zoom_handler_toggles_labels_to_match_the_visible_set`
- `tests/adaptive_labels_periphery.rs:484-496` `the_declutter_holds_its_contract_at_the_edges_and_across_scales`
- `tests/adaptive_labels_periphery.rs:502-514` `a_layered_view_stays_byte_identical_and_a_titled_node_still_names_itself_on_hover`

#### `dup-0345` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/adaptive_labels_periphery.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/adaptive_labels_periphery.rs:522-534` `the_real_concepts_drill_names_its_decluttered_shared_member_on_hover`
- `tests/subject_lens_overlay_client_arms.rs:290-303` `a_lens_flip_with_no_subject_reloads_the_whole_graph_overview`
- `tests/subject_lens_overlay_client_arms.rs:310-323` `a_failed_live_reprojection_fetch_degrades_to_a_message`
- `tests/subject_view_memory_rail_client.rs:207-220` `clicking_a_node_reveals_the_memory_rail_without_touching_the_neighborhood_panel`

#### `dup-0346` (near, 15 sites)

Proposed home: `a new shared module (sites span 12 files: tests/architecture_current_surface.rs, tests/cli.rs, tests/kurrentdb_always_available.rs, tests/kurrentdb_contract_test_surface.rs, tests/meta_phases_declaration_periphery.rs, tests/mutation_scratch_persona_encoding_contract_periphery.rs, tests/phase_of_role_mapping_periphery.rs, tests/readme_retirement_rationale.rs, tests/review_tier_roster_periphery.rs, tests/step_attention_periphery.rs, tests/turbovec_retired.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:49-54` `architecture_text`
- `tests/architecture_current_surface.rs:57-63` `eventstore_source`
- `tests/cli.rs:3155-3160` `main_rs_source`
- `tests/cli.rs:3356-3361` `rigger_js_source`
- `tests/cli.rs:24752-24758` `rust_engineer_persona_text`
- `tests/kurrentdb_always_available.rs:28-32` `manifest_text`
- `tests/kurrentdb_contract_test_surface.rs:38-45` `adapter_source`
- `tests/meta_phases_declaration_periphery.rs:54-59` `rigger_js_source`
- `tests/mutation_scratch_persona_encoding_contract_periphery.rs:37-41` `read_persona`
- `tests/phase_of_role_mapping_periphery.rs:38-43` `rigger_js_source`
- `tests/readme_retirement_rationale.rs:31-34` `readme_text`
- `tests/review_tier_roster_periphery.rs:44-49` `rigger_js_source`
- `tests/step_attention_periphery.rs:964-969` `rigger_js_source`
- `tests/turbovec_retired.rs:34-38` `manifest_text`
- `tests/worker_persona_label_periphery.rs:48-53` `rigger_js_source`

#### `dup-0347` (semantic, 2 sites)

Proposed home: `one shared `architecture_text` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/architecture_current_surface.rs:49-54` `architecture_text`
- `tests/architecture_integrity.rs:50-53` `architecture_text`

#### `dup-0348` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/architecture_current_surface.rs, tests/cli.rs, tests/readme_retirement_rationale.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:152-170` `architecture_names_the_current_store_and_inspector_surface`
- `tests/cli.rs:24806-24824` `rust_engineer_persona_pins_the_mutation_accounting_contract`
- `tests/readme_retirement_rationale.rs:89-107` `readme_records_the_symbols_default_and_the_retirement_rationale`

#### `dup-0349` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/architecture_current_surface.rs, tests/readme_retirement_rationale.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/architecture_current_surface.rs:188-209` `architecture_names_no_retired_or_wrong_default_grounder`
- `tests/readme_retirement_rationale.rs:110-126` `readme_carries_none_of_the_retired_grounder_inversions`

#### `dup-0350` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/batched_fold_cadence.rs, tests/calls_down_execution_path_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/batched_fold_cadence.rs:65-67` `subgraph`
- `tests/batched_fold_cadence.rs:91-93` `subgraph`
- `tests/calls_down_execution_path_periphery.rs:142-144` `subgraph`
- `tests/store_content_identity_periphery.rs:104-106` `subgraph`

#### `dup-0351` (exact, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/batched_fold_cadence.rs, tests/calls_down_execution_path_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/batched_fold_cadence.rs:68-70` `resolve`
- `tests/batched_fold_cadence.rs:94-96` `resolve`
- `tests/calls_down_execution_path_periphery.rs:145-147` `resolve`
- `tests/store_content_identity_periphery.rs:107-109` `resolve`

#### `dup-0352` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/build_budget_slots_periphery.rs, tests/build_env_authority_periphery.rs, tests/store_flag_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_budget_slots_periphery.rs:195-210` `write_workflow`
- `tests/build_env_authority_periphery.rs:165-180` `write_workflow`
- `tests/store_flag_precedence.rs:75-90` `write_workflow`

#### `dup-0353` (semantic, 5 sites)

Proposed home: `one shared `write_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_budget_slots_periphery.rs:195-210` `write_workflow`
- `tests/build_env_authority_periphery.rs:165-180` `write_workflow`
- `tests/scratch_workdir_config.rs:37-39` `write_workflow`
- `tests/store_config.rs:40-42` `write_workflow`
- `tests/store_flag_precedence.rs:75-90` `write_workflow`

#### `dup-0354` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/build_env_authority_periphery.rs, tests/spawn_target_dir_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:215-219` `env_test_lock`
- `tests/spawn_target_dir_periphery.rs:104-108` `spawn_env_test_lock`

#### `dup-0355` (near, 3 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:501-565` `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess`
- `tests/build_env_authority_periphery.rs:641-693` `jobs_cap_coexists_with_a_configured_wrapper_at_both_real_injection_sites`
- `tests/build_env_authority_periphery.rs:720-766` `auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses`

#### `dup-0356` (near, 2 sites)

Proposed home: `build_env_authority_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_env_authority_periphery.rs:932-1007` `run_propagates_a_named_wrappers_uncreatable_cache_dir_at_the_library_entry_point`
- `tests/build_env_authority_periphery.rs:1025-1104` `run_propagates_a_named_wrappers_preexisting_unwritable_cache_dir_at_the_library_entry_point`

#### `dup-0357` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/build_watch_paths.rs, tests/gitsemver_worktree_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/build_watch_paths.rs:78-85` `fixture_repo`
- `tests/gitsemver_worktree_periphery.rs:97-112` `fixture_repo`

#### `dup-0358` (semantic, 3 sites)

Proposed home: `one shared `fixture_repo` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/build_watch_paths.rs:78-85` `fixture_repo`
- `tests/gitsemver_derivation.rs:61-76` `fixture_repo`
- `tests/gitsemver_worktree_periphery.rs:97-112` `fixture_repo`

#### `dup-0359` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/calls_down_execution_path_periphery.rs, tests/community_resolution_knob.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/calls_down_execution_path_periphery.rs:102-106` `node_ids`
- `tests/community_resolution_knob.rs:192-201` `community_nodes`
- `tests/concepts_fold_periphery.rs:80-89` `concept_nodes`

#### `dup-0360` (near, 2 sites)

Proposed home: `calls_down_execution_path_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/calls_down_execution_path_periphery.rs:314-355` `the_depth_bound_clamps_the_layers_the_walk_returns`
- `tests/calls_down_execution_path_periphery.rs:796-864` `the_up_walk_clamps_the_caller_dag_to_the_depth_bound_and_emits_a_deterministic_layered_order`

#### `dup-0361` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:55-120` `spawn`
- `tests/canary_unattributed_rejects_periphery.rs:54-107` `spawn`

#### `dup-0362` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/canary_false_positives_periphery.rs, tests/canary_tolerant_attribution_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:138-145` `panel`
- `tests/canary_tolerant_attribution_periphery.rs:103-110` `panel`
- `tests/canary_unattributed_rejects_periphery.rs:125-132` `panel`

#### `dup-0363` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/canary_false_positives_periphery.rs, tests/canary_findings_volume_periphery.rs, tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:147-161` `item`
- `tests/canary_findings_volume_periphery.rs:133-147` `item`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:65-79` `item`
- `tests/canary_progress_hook_periphery.rs:71-85` `item`
- `tests/canary_unattributed_rejects_periphery.rs:134-148` `item`

#### `dup-0364` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_false_positives_periphery.rs, tests/canary_unattributed_rejects_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_false_positives_periphery.rs:170-297` `run_canary_scores_false_positive_controls_and_project_canary_counts_them`
- `tests/canary_unattributed_rejects_periphery.rs:157-290` `run_canary_scores_an_unattributed_correct_reject_and_project_canary_counts_only_it`

#### `dup-0365` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_item_sharding_jobs_cap_periphery.rs:84-90` `anchor_of`
- `tests/canary_progress_hook_periphery.rs:91-97` `anchor_of`

#### `dup-0366` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/canary_item_sharding_jobs_cap_periphery.rs, tests/canary_progress_hook_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_item_sharding_jobs_cap_periphery.rs:202-236` `spawn`
- `tests/canary_item_sharding_jobs_cap_periphery.rs:288-317` `spawn`
- `tests/canary_progress_hook_periphery.rs:107-134` `spawn`

#### `dup-0367` (near, 12 sites)

Proposed home: `a new shared module (sites span 12 files: tests/canary_model_drift_periphery.rs, tests/cause_wire_periphery.rs, tests/cli.rs, tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs, tests/reset_build_cache_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/spec_lint.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:39-46` `temp_project`
- `tests/cause_wire_periphery.rs:54-61` `temp_project`
- `tests/cli.rs:19-29` `temp_project`
- `tests/graph_show_periphery.rs:49-56` `temp_project`
- `tests/graph_show_staleness.rs:37-44` `temp_project`
- `tests/graph_show_surface.rs:36-43` `temp_project`
- `tests/reset_build_cache_periphery.rs:36-43` `temp_project`
- `tests/reset_menu.rs:37-44` `temp_project`
- `tests/reset_menu_identity_migration_periphery.rs:32-39` `temp_project`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:45-52` `temp_project`
- `tests/spec_lint.rs:23-30` `temp_project`
- `tests/watchdog_cli_periphery.rs:44-51` `temp_project`

#### `dup-0368` (semantic, 19 sites)

Proposed home: `one shared `temp_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 19 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/canary_model_drift_periphery.rs:39-46` `temp_project`
- `tests/cause_wire_periphery.rs:54-61` `temp_project`
- `tests/cli.rs:19-29` `temp_project`
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
- `tests/watchdog_cli_periphery.rs:44-51` `temp_project`

#### `dup-0369` (exact, 13 sites)

Proposed home: `a new shared module (sites span 13 files: tests/canary_model_drift_periphery.rs, tests/cause_wire_periphery.rs, tests/migration_is_deliberate_periphery.rs, tests/reset_build_cache_periphery.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/spec_lint.rs, tests/step_attention_periphery.rs, tests/validate_advisories.rs, tests/validate_behind_the_tree_periphery.rs, tests/watchdog_cli_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:50-65` `run_rigger`
- `tests/cause_wire_periphery.rs:120-132` `run_rigger`
- `tests/migration_is_deliberate_periphery.rs:491-503` `run_rigger`
- `tests/reset_build_cache_periphery.rs:74-86` `run_rigger`
- `tests/reset_derived_compaction_periphery.rs:642-654` `run_rigger`
- `tests/reset_menu.rs:86-98` `run_rigger`
- `tests/reset_menu_identity_migration_periphery.rs:74-86` `run_rigger`
- `tests/spec_lint.rs:35-47` `run_rigger`
- `tests/step_attention_periphery.rs:202-214` `run_rigger`
- `tests/validate_advisories.rs:88-100` `run_rigger`
- `tests/validate_behind_the_tree_periphery.rs:78-90` `run_rigger`
- `tests/watchdog_cli_periphery.rs:110-122` `run_rigger`
- `tests/worktree_liveness_fence_periphery.rs:214-226` `run_rigger`

#### `dup-0370` (near, 2 sites)

Proposed home: `canary_model_drift_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_model_drift_periphery.rs:151-204` `canary_and_validate_treat_an_unattributed_tier_as_unmeasured_never_defaulted_from_output_prose`
- `tests/canary_model_drift_periphery.rs:213-293` `canary_and_validate_drift_reads_only_metadata_prose_can_neither_mask_nor_forge_it`

#### `dup-0371` (near, 2 sites)

Proposed home: `canary_tolerant_attribution_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/canary_tolerant_attribution_periphery.rs:130-155` `a_tolerant_match_in_a_later_about_entry_still_scores_the_catch`
- `tests/canary_tolerant_attribution_periphery.rs:166-190` `an_empty_about_entry_never_scores_a_catch_even_against_a_trailing_slash_anchor`

#### `dup-0372` (exact, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/cause_wire_periphery.rs, tests/cli.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:65-69` `seed_store`
- `tests/cli.rs:38-42` `seed_store`
- `tests/heartbeat_write_read_agree_periphery.rs:118-122` `seed_store`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:56-60` `seed_store`
- `tests/watchdog_cli_periphery.rs:56-60` `seed_store`

#### `dup-0373` (near, 16 sites)

Proposed home: `a new shared module (sites span 16 files: tests/cause_wire_periphery.rs, tests/change_path_revert_periphery.rs, tests/cli.rs, tests/dedup_seeding_periphery.rs, tests/migration_is_deliberate_periphery.rs, tests/projections_stay_local.rs, tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs, tests/step_attention_periphery.rs, tests/store_resolution.rs, tests/validate_advisories.rs, tests/watchdog_cli_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:75-97` `run_stream_identity`
- `tests/change_path_revert_periphery.rs:130-155` `run_stream_identity`
- `tests/cli.rs:49-71` `run_stream_identity`
- `tests/dedup_seeding_periphery.rs:580-605` `run_stream_identity`
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

#### `dup-0374` (semantic, 18 sites)

Proposed home: `one shared `run_stream_identity` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 18 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cause_wire_periphery.rs:75-97` `run_stream_identity`
- `tests/change_path_revert_periphery.rs:130-155` `run_stream_identity`
- `tests/cli.rs:49-71` `run_stream_identity`
- `tests/dedup_seeding_periphery.rs:580-605` `run_stream_identity`
- `tests/graph_show_periphery.rs:61-77` `run_stream_identity`
- `tests/graph_show_staleness.rs:49-65` `run_stream_identity`
- `tests/graph_show_surface.rs:48-64` `run_stream_identity`
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

#### `dup-0375` (near, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/cause_wire_periphery.rs, tests/heartbeat_write_read_agree_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:103-116` `seed_run_events`
- `tests/heartbeat_write_read_agree_periphery.rs:151-164` `seed_run_events`
- `tests/reset_derived_live_writer_guard_periphery.rs:91-103` `seed_run_events`
- `tests/reset_menu.rs:107-119` `seed_run_events`
- `tests/reset_menu_identity_migration_periphery.rs:95-107` `seed_run_events`
- `tests/watchdog_cli_periphery.rs:93-106` `seed_run_events`

#### `dup-0376` (semantic, 8 sites)

Proposed home: `one shared `seed_run_events` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 8 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cause_wire_periphery.rs:103-116` `seed_run_events`
- `tests/cli.rs:82-100` `seed_run_events`
- `tests/heartbeat_write_read_agree_periphery.rs:151-164` `seed_run_events`
- `tests/reset_derived_live_writer_guard_periphery.rs:91-103` `seed_run_events`
- `tests/reset_menu.rs:107-119` `seed_run_events`
- `tests/reset_menu_identity_migration_periphery.rs:95-107` `seed_run_events`
- `tests/step_attention_periphery.rs:178-196` `seed_run_events`
- `tests/watchdog_cli_periphery.rs:93-106` `seed_run_events`

#### `dup-0377` (near, 7 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cause_wire_periphery.rs, tests/cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cause_wire_periphery.rs:195-217` `a_legacy_causeless_unit_failed_event_survives_a_real_store_round_trip_and_renders_unknown`
- `tests/cause_wire_periphery.rs:224-256` `the_reject_recurrence_line_names_the_latest_of_several_recorded_causes_through_the_real_binary`
- `tests/cli.rs:9591-9629` `stats_reports_the_latest_run_by_default_and_all_for_the_aggregate`
- `tests/cli.rs:20536-20595` `release_ready_handoff_surfaces_on_status_for_a_done_run`
- `tests/cli.rs:20805-20846` `release_ready_is_silent_on_status_for_an_unfinished_run`
- `tests/cli.rs:20917-20950` `release_ready_pluralizes_the_unit_count_on_status_for_a_multi_unit_run`
- `tests/cli.rs:20995-21017` `release_ready_is_silent_on_status_for_a_spec_defective_run`

#### `dup-0378` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:58-72` `run_rigger`
- `tests/dedup_seeding_periphery.rs:655-671` `run_rigger`

#### `dup-0379` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:77-83` `ingested_count`
- `tests/dedup_seeding_periphery.rs:676-682` `ingested_count`

#### `dup-0380` (semantic, 2 sites)

Proposed home: `one shared `ingested_count` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/change_path_revert_periphery.rs:77-83` `ingested_count`
- `tests/dedup_seeding_periphery.rs:676-682` `ingested_count`

#### `dup-0381` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/change_path_revert_periphery.rs, tests/dedup_seeding_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/change_path_revert_periphery.rs:159-175` `read_run_stream`
- `tests/dedup_seeding_periphery.rs:630-646` `read_run_stream`

#### `dup-0382` (semantic, 3 sites)

Proposed home: `one shared `read_run_stream` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/change_path_revert_periphery.rs:159-175` `read_run_stream`
- `tests/dedup_seeding_periphery.rs:630-646` `read_run_stream`
- `tests/reset_derived_compaction_periphery.rs:2835-2839` `read_run_stream`

#### `dup-0383` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:82-100` `seed_run_events`
- `tests/step_attention_periphery.rs:178-196` `seed_run_events`

#### `dup-0384` (semantic, 4 sites)

Proposed home: `one shared `temp_git_project_with_commit` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:105-122` `temp_git_project_with_commit`
- `tests/step_attention_periphery.rs:464-484` `temp_git_project_with_commit`
- `tests/workflow_driver_resolved_model_periphery.rs:57-78` `temp_git_project_with_commit`
- `tests/worktree_liveness_fence_periphery.rs:94-115` `temp_git_project_with_commit`

#### `dup-0385` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:126-136` `git_out`
- `tests/worktree_liveness_fence_periphery.rs:149-159` `git_out`

#### `dup-0386` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/reset_derived_compaction.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:139-141` `run_rigger`
- `tests/reset_derived_compaction.rs:82-84` `run_rigger`

#### `dup-0387` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/cli.rs, tests/reset_derived_compaction.rs, tests/reset_derived_live_writer_guard_periphery.rs, tests/spawn_scratch_reap_authorized_root_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:146-172` `run_rigger_envs`
- `tests/reset_derived_compaction.rs:86-101` `run_rigger_envs`
- `tests/reset_derived_live_writer_guard_periphery.rs:108-123` `run_rigger`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:118-133` `run_rigger_envs`

#### `dup-0388` (semantic, 3 sites)

Proposed home: `one shared `run_rigger_envs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:146-172` `run_rigger_envs`
- `tests/reset_derived_compaction.rs:86-101` `run_rigger_envs`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:118-133` `run_rigger_envs`

#### `dup-0389` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:189-197` `git_ok`
- `tests/worktree_liveness_fence_periphery.rs:138-146` `git_ok`

#### `dup-0390` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:352-375` `emit_review_finding_shows_in_peers`
- `tests/cli.rs:7632-7735` `step_surfaces_a_hung_unbounded_spawn_recorded_as_a_liveness_fault_by_the_driver`
- `tests/cli.rs:7760-7823` `step_attention_never_restamps_a_hung_unbounded_spawn_when_repo_less`
- `tests/cli.rs:11104-11168` `a_liveness_fault_on_a_review_spawn_halts_instead_of_re_parking`
- `tests/cli.rs:14363-14393` `result_if_absent_records_when_the_spawn_is_unanswered`
- `tests/cli.rs:14401-14439` `result_if_absent_never_clobbers_a_self_reported_success`

#### `dup-0391` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:871-1008` `reset_runs_compacts_the_on_disk_graph_after_reclaiming_superseded_rows`
- `tests/cli.rs:1038-1183` `reset_runs_reports_nonzero_bytes_reclaimed_then_a_second_pass_is_an_idempotent_no_op`

#### `dup-0392` (semantic, 2 sites)

Proposed home: `one shared `reported_reclaimed_bytes` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:1014-1020` `reported_reclaimed_bytes`
- `tests/reset_derived_compaction_periphery.rs:4409-4422` `reported_reclaimed_bytes`

#### `dup-0393` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1372-1389` `prompt_refuses_to_fabricate_a_store_when_none_exists`
- `tests/cli.rs:1433-1450` `scratch_refuses_to_fabricate_a_store_when_none_exists`

#### `dup-0394` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1792-1815` `result_if_absent_orphan_advisory_states_the_conditional_not_a_recording`
- `tests/cli.rs:15786-15806` `canary_if_model_changed_skips_when_the_model_is_unchanged`
- `tests/cli.rs:15813-15835` `canary_if_model_changed_runs_when_a_tier_resolved_model_repointed`
- `tests/cli.rs:15844-15878` `canary_if_model_changed_skips_a_snapshot_only_date_suffix_bump_without_running_the_panel`

#### `dup-0395` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:1880-1953` `a_spawns_scratch_is_reclaimed_the_moment_its_result_is_recorded_for_every_outcome`
- `tests/cli.rs:1969-2043` `a_spawns_mutation_scratch_is_reclaimed_the_moment_its_own_result_reports_for_every_outcome`

#### `dup-0396` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2057-2092` `a_reviewers_result_never_reclaims_the_implementers_mutation_scratch`
- `tests/cli.rs:2105-2148` `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots`
- `tests/cli.rs:2165-2212` `a_dotdot_spawn_id_never_escapes_the_pre_existing_agent_scratch_root_either`
- `tests/cli.rs:2233-2271` `a_leading_slash_spawn_id_never_collapses_the_reclaim_to_its_registered_root`

#### `dup-0397` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2361-2409` `two_speculation_lanes_of_the_same_unit_get_distinct_mutation_scratch_dirs`
- `tests/cli.rs:8502-8566` `a_terminal_units_registered_mutation_scratch_is_reaped_while_a_live_siblings_survives`
- `tests/cli.rs:9332-9449` `a_resumed_run_reaps_an_escalated_and_an_on_pass_none_settled_units_registered_mutation_scratch_not_just_an_integrated_ones`

#### `dup-0398` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:2417-2428` `write_grounder_workflow`
- `tests/cli.rs:16081-16096` `write_gating_lint_project`

#### `dup-0399` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3179-3235` `worktree_sweep_completes_before_any_add_within_one_step`
- `tests/cli.rs:7841-7876` `the_hung_cursor_is_persisted_only_after_the_step_that_carries_it_is_printed`

#### `dup-0400` (semantic, 6 sites)

Proposed home: `one shared `rigger_js_source` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 6 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:3356-3361` `rigger_js_source`
- `tests/meta_phases_declaration_periphery.rs:54-59` `rigger_js_source`
- `tests/phase_of_role_mapping_periphery.rs:38-43` `rigger_js_source`
- `tests/review_tier_roster_periphery.rs:44-49` `rigger_js_source`
- `tests/step_attention_periphery.rs:964-969` `rigger_js_source`
- `tests/worker_persona_label_periphery.rs:48-53` `rigger_js_source`

#### `dup-0401` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3481-3537` `native_driver_scratch_policy_directs_the_worker_to_its_own_spawn_owned_container`
- `tests/worker_persona_label_periphery.rs:413-434` `workerlabel_is_actually_wired_into_runworkers_agent_call_label`

#### `dup-0402` (near, 15 sites)

Proposed home: `a new shared module (sites span 4 files: tests/cli.rs, tests/step_attention_periphery.rs, tests/workflow_driver_resolved_model_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:3617-3641` `write_two_stage_workflow`
- `tests/cli.rs:3646-3670` `write_budget_one_two_stage_workflow`
- `tests/cli.rs:3756-3776` `write_standalone_review_workflow`
- `tests/cli.rs:4173-4197` `write_reviewless_git_unit_workflow`
- `tests/cli.rs:4204-4229` `write_reviewless_git_escalating_unit_workflow`
- `tests/cli.rs:6912-6937` `write_failing_gate_escalating_workflow`
- `tests/cli.rs:6946-6971` `write_manual_review_workflow`
- `tests/cli.rs:7174-7197` `write_budget_one_dependency_workflow`
- `tests/cli.rs:7264-7277` `write_liveness_workflow`
- `tests/cli.rs:7605-7618` `write_unbounded_liveness_workflow`
- `tests/cli.rs:10799-10830` `write_gated_reviewed_workflow`
- `tests/step_attention_periphery.rs:221-246` `write_attention_progression_workflow`
- `tests/step_attention_periphery.rs:492-518` `write_attention_ordering_workflow`
- `tests/workflow_driver_resolved_model_periphery.rs:85-98` `write_one_stage_workflow`
- `tests/worktree_liveness_fence_periphery.rs:234-258` `write_reviewless_git_unit_workflow`

#### `dup-0403` (semantic, 2 sites)

Proposed home: `one shared `write_reviewless_git_unit_workflow` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:4173-4197` `write_reviewless_git_unit_workflow`
- `tests/worktree_liveness_fence_periphery.rs:234-258` `write_reviewless_git_unit_workflow`

#### `dup-0404` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4244-4319` `step_reclaims_the_units_worktree_and_deletes_its_branch_on_a_clean_integrate`
- `tests/cli.rs:4334-4402` `step_reclaims_the_units_worktree_but_keeps_its_branch_on_a_terminal_escalation`

#### `dup-0405` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:4430-4638` `step_restores_the_unit_worktree_a_gate_deletes_before_the_review_spawn`
- `tests/cli.rs:4864-5111` `step_stamps_a_real_reviewed_sha_after_repeated_between_step_deletions`
- `tests/cli.rs:5294-5437` `step_stamps_a_real_failed_sha_after_a_deletion_before_the_reject_stamp`

#### `dup-0406` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:5477-5639` `run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review`
- `tests/cli.rs:5924-6077` `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored_end_to_end`
- `tests/cli.rs:8841-8980` `a_speculation_winners_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_winner_integrate_teardown`
- `tests/cli.rs:8994-9132` `a_speculation_escalations_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_escalation_tail_teardown`
- `tests/cli.rs:9155-9305` `a_speculation_on_pass_none_winners_registered_mutation_scratch_across_all_lanes_is_reaped_by_the_real_on_pass_none_exit_teardown`

#### `dup-0407` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:6101-6248` `resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_exhaustive_gates_own_deletion_is_restored`
- `tests/cli.rs:6271-6450` `resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_post_merge_re_gates_own_deletion_is_restored`

#### `dup-0408` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:6758-6819` `run_registers_a_credential_free_shared_instance`
- `tests/cli.rs:6839-6904` `run_driver_workflow_registers_a_credential_free_shared_instance`

#### `dup-0409` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:6981-7073` `step_carries_the_escalated_set_when_a_fixpoint_is_reached_with_a_wedged_unit`
- `tests/step_attention_periphery.rs:252-351` `recurrence_and_stalled_frontier_survive_real_process_boundaries`

#### `dup-0410` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:7282-7292` `plant_stale_marker`
- `tests/step_attention_periphery.rs:416-426` `plant_stale_marker`

#### `dup-0411` (semantic, 2 sites)

Proposed home: `one shared `plant_stale_marker` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:7282-7292` `plant_stale_marker`
- `tests/step_attention_periphery.rs:416-426` `plant_stale_marker`

#### `dup-0412` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8177-8225` `run_teardown_reclaims_run_level_scratch_at_a_definition_drift_halt`
- `tests/cli.rs:8242-8298` `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_hung_spawn_may_be_alive`

#### `dup-0413` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:8321-8348` `run_teardown_spares_run_level_scratch_at_a_manual_review_pause`
- `tests/cli.rs:8373-8421` `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_manual_review_is_pending`
- `tests/cli.rs:8431-8479` `run_teardown_reclaims_run_level_scratch_after_a_manual_review_is_integrated`

#### `dup-0414` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:9676-9750` `stats_cli_renders_exact_per_role_spawn_timing_and_unpaired_disclosure`
- `tests/cli.rs:9824-9875` `stats_cli_excludes_suspect_non_positive_duration_pairs_as_unpaired_not_zero`

#### `dup-0415` (exact, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:10201-10212` `step_rejects_an_unknown_flag`
- `tests/cli.rs:10251-10262` `step_rejects_base_without_a_value`
- `tests/watchdog_cli_periphery.rs:356-370` `watch_rejects_an_unknown_flag_through_the_real_binary_with_a_nonzero_exit`

#### `dup-0416` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:10218-10246` `step_accepts_base_and_anchors_the_run_branch`
- `tests/cli.rs:10271-10315` `step_creates_run_branch_off_head_when_base_unresolvable`

#### `dup-0417` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:10325-10355` `step_refuses_when_there_is_no_reachable_base`
- `tests/cli.rs:10366-10397` `run_refuses_when_there_is_no_reachable_base`
- `tests/cli.rs:10407-10450` `run_workflow_refuses_when_there_is_no_reachable_base`

#### `dup-0418` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/cli.rs, tests/step_attention_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:10787-10789` `temp_repoless_project`
- `tests/step_attention_periphery.rs:143-145` `temp_repoless_project`

#### `dup-0419` (semantic, 2 sites)

Proposed home: `one shared `temp_repoless_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:10787-10789` `temp_repoless_project`
- `tests/step_attention_periphery.rs:143-145` `temp_repoless_project`

#### `dup-0420` (exact, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11404-11421` `write_gated_workflow_no_review`
- `tests/cli.rs:11473-11490` `write_reviewed_workflow_no_gate`
- `tests/cli.rs:11545-11565` `write_reviewed_workflow_added_gate`
- `tests/cli.rs:11621-11643` `write_reviewed_workflow_extra_stage`

#### `dup-0421` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11430-11466` `replay_candidate_column_reacts_to_a_changed_config`
- `tests/cli.rs:11499-11539` `replay_removing_a_gate_lowers_the_candidate_gate_runs`

#### `dup-0422` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:11577-11616` `replay_an_added_gate_fails_safe_and_never_fabricates_a_pass`
- `tests/cli.rs:11652-11684` `replay_an_uncovered_candidate_spawn_parks_and_still_prints_a_partial_column`

#### `dup-0423` (near, 7 sites)

Proposed home: `cli::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12029-12047` `validate_reports_none_when_auto_finds_no_known_wrapper_on_path`
- `tests/cli.rs:12052-12069` `validate_reports_the_resolved_wrapper_when_auto_finds_a_known_wrapper_on_path`
- `tests/cli.rs:12076-12106` `validate_reports_cache_dir_and_budget_alongside_the_wrapper`
- `tests/cli.rs:12248-12273` `validate_reports_none_when_autos_discovered_wrapper_has_an_uncreatable_cache_dir`
- `tests/cli.rs:12344-12370` `validate_reports_none_when_autos_discovered_wrapper_has_a_preexisting_unwritable_cache_dir`
- `tests/cli.rs:12463-12480` `validate_reports_mutation_on_when_cargo_mutants_is_resolvable`
- `tests/cli.rs:12486-12504` `validate_reports_mutation_off_without_probing_path_when_configured_off`

#### `dup-0424` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12211-12240` `validate_fails_at_run_start_when_a_named_wrappers_cache_dir_cannot_be_created`
- `tests/cli.rs:12305-12334` `validate_fails_at_run_start_when_a_named_wrappers_cache_dir_is_preexisting_but_unwritable`

#### `dup-0425` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12399-12421` `validate_fails_at_run_start_when_mutation_is_on_and_cargo_mutants_is_absent_from_path`
- `tests/cli.rs:12437-12458` `validate_fails_before_any_output_when_mutation_is_on_and_cargo_mutants_is_absent`

#### `dup-0426` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12857-12899` `validate_footprint_registered_scratch_roots_measures_the_real_mutation_scratch_root`
- `tests/cli.rs:13275-13385` `validate_footprint_worktrees_and_per_unit_caches_measure_real_dead_and_live_entries_through_the_binary`

#### `dup-0427` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:12921-13047` `validate_flags_registered_scratch_roots_dead_share_scoped_to_real_spawn_liveness_in_the_store`
- `tests/cli.rs:13067-13163` `validate_flags_a_prior_abandoned_runs_orphan_even_when_a_later_run_reuses_the_identical_spawn_id`

#### `dup-0428` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:13941-14009` `installed_workflow_courier_prompt_is_foreground_and_honest`
- `tests/cli.rs:14029-14107` `installed_workflow_courier_waits_on_an_auto_backgrounded_step`
- `tests/cli.rs:14284-14356` `installed_workflow_driver_guards_a_null_step`

#### `dup-0429` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:14646-14713` `step_halts_on_definition_drift_and_rebase_definition_continues`
- `tests/cli.rs:14719-14760` `a_fresh_run_repins_the_current_definition_and_never_halts`

#### `dup-0430` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:14890-14947` `stats_canary_reports_the_findings_raised_total_summed_across_items`
- `tests/cli.rs:14982-15036` `stats_canary_renders_na_for_a_tier_with_an_unattributed_correct_reject`
- `tests/cli.rs:15047-15096` `stats_canary_still_renders_a_genuine_zero_when_every_reject_has_attribution`
- `tests/cli.rs:15117-15172` `stats_canary_reports_the_control_false_positive_line`
- `tests/cli.rs:15481-15542` `stats_canary_reports_the_model_pinning_header_through_a_real_wire_event`

#### `dup-0431` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:15264-15297` `rigger_help_gives_the_jobs_flag_its_own_description_line_through_the_real_binary`
- `tests/cli.rs:15312-15370` `rigger_help_gives_the_model_flag_its_own_description_line_through_the_real_binary`

#### `dup-0432` (near, 5 sites)

Proposed home: `a new shared module (sites span 3 files: tests/cli.rs, tests/migration_is_deliberate_periphery.rs, tests/validate_advisories.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:15597-15646` `validate_warns_when_a_tier_resolved_model_repointed_between_runs`
- `tests/cli.rs:15653-15680` `validate_advises_softly_on_a_snapshot_only_date_suffix_bump`
- `tests/cli.rs:15720-15780` `validate_detects_a_stream_whose_position_order_and_revision_order_disagree`
- `tests/migration_is_deliberate_periphery.rs:566-591` `validate_warns_of_retired_code_entities_with_the_measured_count_and_never_fails`
- `tests/validate_advisories.rs:271-296` `validate_warns_of_log_bloat_with_the_measured_factor_and_names_reset_derived`

#### `dup-0433` (semantic, 2 sites)

Proposed home: `one shared `seed_order_signature` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/cli.rs:15692-15713` `seed_order_signature`
- `tests/watchdog_cli_periphery.rs:224-246` `seed_order_signature`

#### `dup-0434` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16775-16825` `status_reports_not_serving_when_the_recorded_marker_names_a_dead_dash`
- `tests/cli.rs:16838-16883` `status_never_names_the_unattributed_pid_sentinel_as_a_dead_process`

#### `dup-0435` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:16894-16953` `status_shows_the_url_when_the_recorded_marker_names_a_genuinely_serving_dash`
- `tests/cli.rs:16967-17034` `status_trusts_a_genuinely_alive_url_even_with_a_mismatched_marker`
- `tests/cli.rs:17050-17128` `status_reports_not_serving_when_a_mismatched_marker_leaves_a_dead_url_unverified`

#### `dup-0436` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:17465-17527` `docs_ships_graph_hygiene_guidance_to_consumers`
- `tests/cli.rs:17542-17585` `docs_ships_three_verb_lookup_guidance_to_consumers`

#### `dup-0437` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:17596-17668` `validate_fails_when_the_committed_using_rigger_docs_drift_and_passes_when_in_sync`
- `tests/cli.rs:17682-17735` `validate_docs_drift_gate_covers_the_second_registry_entry`
- `tests/cli.rs:17832-17886` `validate_docs_drift_gate_covers_the_planning_field_guide_page`

#### `dup-0438` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18063-18106` `docs_renders_every_per_operation_skill_through_the_compiled_binary`
- `tests/cli.rs:18286-18381` `docs_renders_every_watching_discipline_skill_through_the_compiled_binary`

#### `dup-0439` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18120-18179` `validate_docs_drift_gate_covers_each_per_operation_skill`
- `tests/cli.rs:18395-18462` `validate_docs_drift_gate_covers_each_watching_discipline_skill`

#### `dup-0440` (exact, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18193-18254` `setup_installs_every_per_operation_skill_into_the_consumer_project`
- `tests/cli.rs:18476-18537` `setup_installs_every_watching_discipline_skill_into_the_consumer_project`

#### `dup-0441` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18630-18668` `watch_once_never_names_the_unattributed_pid_sentinel_when_no_url_is_recorded`
- `tests/cli.rs:25256-25294` `watch_once_never_names_the_unattributed_pid_sentinel_when_the_url_is_unparseable`

#### `dup-0442` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18688-18721` `watch_once_reports_a_dead_dash_when_only_the_url_breadcrumb_is_recorded_and_no_marker_exists`
- `tests/cli.rs:25316-25345` `watch_once_parses_the_urls_port_past_a_colon_in_the_path_with_no_marker`

#### `dup-0443` (near, 5 sites)

Proposed home: `cli::support (consolidate these 5 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:18801-18843` `watch_once_reports_no_dash_anomaly_for_a_done_run_even_with_a_dead_marker`
- `tests/cli.rs:18860-18920` `watch_once_reports_no_dash_anomaly_for_a_fresh_run_that_inherits_an_earlier_runs_dead_marker`
- `tests/cli.rs:18960-19009` `watch_once_reports_this_runs_own_dead_marker_when_written_after_its_run_started`
- `tests/cli.rs:19031-19083` `watch_once_reports_a_dead_marker_predating_run_started_when_dash_attempt_names_this_run`
- `tests/cli.rs:19112-19164` `watch_once_suppresses_a_predating_marker_when_dash_attempt_names_a_different_run`

#### `dup-0444` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19246-19262` `stage_rigger_shim`
- `tests/cli.rs:19667-19687` `stage_failing_docs_rigger_shim`
- `tests/cli.rs:19715-19743` `stage_stale_rigger_shim`

#### `dup-0445` (near, 6 sites)

Proposed home: `cli::support (consolidate these 6 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19430-19472` `setup_precommit_hook_passes_untouched_when_the_render_matches`
- `tests/cli.rs:19487-19558` `setup_precommit_hook_never_drift_checks_or_stages_a_registry_entry_outside_its_scope`
- `tests/cli.rs:19755-19805` `setup_precommit_hook_prefers_the_trees_own_built_binary_over_a_stale_path_rigger`
- `tests/cli.rs:19815-19865` `setup_precommit_hook_refuses_the_same_commit_shape_with_only_a_stale_path_rigger`
- `tests/cli.rs:20192-20223` `setup_precommit_hook_warns_and_proceeds_when_rigger_is_unavailable`
- `tests/cli.rs:20230-20260` `setup_precommit_hook_warns_and_proceeds_when_rigger_docs_errors`

#### `dup-0446` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:19694-19705` `stage_tree_built_binary`
- `tests/cli.rs:19872-19887` `stage_unit_derived_binary`

#### `dup-0447` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:21370-21415` `step_self_heals_a_stale_marker_naming_a_dead_pid`
- `tests/cli.rs:21440-21486` `step_self_heals_a_stale_marker_naming_a_live_pid_whose_port_is_unserved`

#### `dup-0448` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:22080-22091` `write_live_instance`
- `tests/cli.rs:22097-22108` `write_stale_instance`

#### `dup-0449` (near, 7 sites)

Proposed home: `cli::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:22121-22209` `a_reap_on_idle_singleton_serves_while_an_instance_heartbeats_then_reaps_when_the_registry_empties`
- `tests/cli.rs:22325-22409` `a_reap_on_idle_singleton_does_not_reap_before_any_instance_has_registered`
- `tests/cli.rs:22436-22540` `a_reap_on_idle_singleton_survives_a_fresh_agent_liveness_marker_after_the_registry_ages_out`
- `tests/cli.rs:22558-22663` `a_reap_on_idle_singleton_survives_a_fresh_agent_liveness_marker_with_no_git_repo_at_launch`
- `tests/cli.rs:22678-22789` `a_reap_on_idle_singleton_survives_a_second_registered_projects_fresh_agent_liveness_marker`
- `tests/cli.rs:22812-22932` `a_reap_on_idle_singleton_survives_a_foreign_agent_liveness_marker_whose_own_registry_entry_was_already_stale_before_the_watchers_first_poll`
- `tests/cli.rs:22948-23087` `a_landing_poll_racing_the_watchers_first_tick_does_not_erase_a_foreign_projects_only_route_into_known_roots`

#### `dup-0450` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:23094-23149` `a_dash_without_reap_on_idle_never_self_reaps_on_a_quiet_machine`
- `tests/cli.rs:23164-23228` `a_reap_on_idle_singleton_in_a_homeless_environment_serves_without_a_watcher`

#### `dup-0451` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:24908-24950` `watch_once_never_names_a_mismatched_markers_pid_for_the_recorded_urls_port`
- `tests/cli.rs:25374-25423` `watch_once_never_names_a_mismatched_markers_pid_when_the_urls_path_contains_a_colon`

#### `dup-0452` (near, 4 sites)

Proposed home: `cli::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:25562-25600` `run_given_a_spec_path_names_the_spec_lint_as_a_next_step`
- `tests/cli.rs:25725-25768` `step_given_a_spec_path_names_the_spec_lint_even_when_the_step_then_refuses_for_no_reachable_base`
- `tests/cli.rs:26042-26077` `step_reminder_prints_despite_env_naming_a_foreign_pid`
- `tests/cli.rs:26121-26155` `run_reminder_prints_despite_env_naming_a_foreign_pid`

#### `dup-0453` (near, 3 sites)

Proposed home: `cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:25788-25848` `run_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`
- `tests/cli.rs:25857-25896` `step_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`
- `tests/cli.rs:25914-25981` `run_driver_workflow_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site`

#### `dup-0454` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26001-26037` `step_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid`
- `tests/cli.rs:26081-26116` `run_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid`

#### `dup-0455` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26219-26268` `run_driver_workflow_prints_the_spec_lint_reminder_and_honors_the_pid_scoped_dedup`
- `tests/cli.rs:26280-26335` `run_driver_workflow_reminder_never_reaches_stdout_in_any_pid_sentinel_direction`

#### `dup-0456` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26351-26385` `run_driver_workflow_fresh_notice_never_reaches_stdout`
- `tests/cli.rs:26392-26416` `run_driver_cli_fresh_notice_still_prints_on_stdout`

#### `dup-0457` (near, 2 sites)

Proposed home: `cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/cli.rs:26508-26608` `cmd_dash_gives_the_stopped_listener_diagnosis_naming_resume_or_kill`
- `tests/cli.rs:26687-26802` `step_names_the_stopped_holder_when_the_step_paths_own_auto_start_hits_the_predecessor_scenario`

#### `dup-0458` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_entity_test_exclusion_periphery.rs, tests/symbol_ref_caller_attribution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:98-136` `is_test_false_serializes_byte_identically_to_the_pre86_form`
- `tests/code_entity_test_exclusion_periphery.rs:262-289` `is_out_of_line_module_false_serializes_byte_identically_to_the_pre_round6_form`
- `tests/code_entity_test_exclusion_periphery.rs:377-404` `path_override_none_serializes_byte_identically_to_the_pre_round7_form`
- `tests/code_entity_test_exclusion_periphery.rs:498-525` `enclosing_inline_module_path_none_serializes_byte_identically_to_the_pre_round9_form`
- `tests/symbol_ref_caller_attribution.rs:32-77` `a_caller_less_reference_serializes_byte_identically_to_the_pre37_form`

#### `dup-0459` (near, 4 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:139-193` `is_test_true_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:292-329` `is_out_of_line_module_true_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:407-445` `path_override_some_serializes_the_key_and_round_trips`
- `tests/code_entity_test_exclusion_periphery.rs:528-566` `enclosing_inline_module_path_some_serializes_the_key_and_round_trips`

#### `dup-0460` (near, 5 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_entity_test_exclusion_periphery.rs, tests/symbol_ref_caller_attribution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:196-248` `a_pre86_persisted_index_with_no_is_test_key_loads_defaulting_every_item_to_false`
- `tests/code_entity_test_exclusion_periphery.rs:332-368` `a_pre_round6_persisted_index_with_no_is_out_of_line_module_key_loads_defaulting_to_false`
- `tests/code_entity_test_exclusion_periphery.rs:448-486` `a_pre_round7_persisted_index_with_no_path_override_key_loads_defaulting_to_none`
- `tests/code_entity_test_exclusion_periphery.rs:569-612` `a_pre_round9_persisted_index_with_no_enclosing_inline_module_path_key_loads_defaulting_to_none`
- `tests/symbol_ref_caller_attribution.rs:130-180` `a_pre37_persisted_index_loads_folding_references_caller_less`

#### `dup-0461` (near, 4 sites)

Proposed home: `code_entity_test_exclusion_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_entity_test_exclusion_periphery.rs:752-799` `cfg_not_test_and_cfg_attr_predicates_graph_as_product_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:808-840` `a_not_wrapping_a_non_test_atom_graphs_as_product_through_the_public_api`
- `tests/code_entity_test_exclusion_periphery.rs:1024-1059` `a_url_bearing_attribute_between_test_and_the_item_does_not_leak_the_item_into_the_graph`
- `tests/code_entity_test_exclusion_periphery.rs:1140-1173` `a_comment_mentioning_test_attribute_text_does_not_exclude_the_item_through_the_public_api`

#### `dup-0462` (near, 26 sites)

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

#### `dup-0463` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/code_ingest_events.rs, tests/community_detection_pass.rs, tests/community_fold_periphery.rs, tests/community_resolution_knob.rs, tests/concepts_fold_periphery.rs, tests/design_intent_events.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:30-34` `apply_json`
- `tests/community_detection_pass.rs:31-35` `apply_json`
- `tests/community_fold_periphery.rs:39-43` `apply_json`
- `tests/community_resolution_knob.rs:62-66` `apply_json`
- `tests/concepts_fold_periphery.rs:42-46` `apply_json`
- `tests/design_intent_events.rs:38-42` `apply_json`

#### `dup-0464` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:241-304` `real_extraction_tiers_every_structural_edge_through_the_emit_fold_pipeline`
- `tests/code_ingest_events.rs:308-378` `real_extraction_folds_caller_attributed_calls_edges_at_every_tier`

#### `dup-0465` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:382-451` `re_extracting_a_file_that_drops_a_call_supersedes_its_calls_edge_end_to_end`
- `tests/code_ingest_events.rs:711-793` `re_extracting_a_changed_file_supersedes_its_removed_symbols_end_to_end`

#### `dup-0466` (near, 9 sites)

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

#### `dup-0467` (near, 2 sites)

Proposed home: `code_ingest_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_ingest_events.rs:1129-1170` `a_refs_only_files_re_extraction_supersedes_via_the_reference_batch_boundary`
- `tests/code_ingest_events.rs:1173-1227` `a_re_extraction_supersedes_only_its_own_files_edges_not_another_files_reference`

#### `dup-0468` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_overview_collapse_viz.rs, tests/subject_lens_overlay_served_page.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:74-88` `build_harness`
- `tests/subject_lens_overlay_served_page.rs:550-564` `build_additive_harness`

#### `dup-0469` (semantic, 5 sites)

Proposed home: `one shared `build_harness` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/code_lens_overview_collapse_viz.rs:74-88` `build_harness`
- `tests/metadata_card_handoff_viz.rs:123-138` `build_harness`
- `tests/proof_row_renders_on_the_card.rs:76-91` `build_harness`
- `tests/subject_lens_overlay_client_arms.rs:82-97` `build_harness`
- `tests/subject_view_memory_rail_client.rs:100-115` `build_harness`

#### `dup-0470` (exact, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/code_lens_overview_collapse_viz.rs, tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_lens_overlay_served_page.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:92-118` `run_node_harness`
- `tests/metadata_card_handoff_viz.rs:141-168` `run_node_harness`
- `tests/proof_row_renders_on_the_card.rs:94-121` `run_node_harness`
- `tests/subject_lens_overlay_client_arms.rs:101-127` `run_node_harness`
- `tests/subject_lens_overlay_served_page.rs:59-85` `run_node_harness`
- `tests/subject_view_memory_rail_client.rs:119-145` `run_node_harness`

#### `dup-0471` (semantic, 6 sites)

Proposed home: `one shared `run_node_harness` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 6 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/code_lens_overview_collapse_viz.rs:92-118` `run_node_harness`
- `tests/metadata_card_handoff_viz.rs:141-168` `run_node_harness`
- `tests/proof_row_renders_on_the_card.rs:94-121` `run_node_harness`
- `tests/subject_lens_overlay_client_arms.rs:101-127` `run_node_harness`
- `tests/subject_lens_overlay_served_page.rs:59-85` `run_node_harness`
- `tests/subject_view_memory_rail_client.rs:119-145` `run_node_harness`

#### `dup-0472` (exact, 6 sites)

Proposed home: `a new shared module (sites span 4 files: tests/code_lens_overview_collapse_viz.rs, tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_served_page.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_overview_collapse_viz.rs:200-213` `the_overview_collapses_to_sized_labelled_community_super_nodes_purely`
- `tests/metadata_card_handoff_viz.rs:297-310` `metadata_card_renders_every_taxonomy_and_chips_hand_off_to_their_own_lens`
- `tests/metadata_card_handoff_viz.rs:383-396` `metadata_card_wiring_fires_at_every_render_and_drill_call_site`
- `tests/proof_row_renders_on_the_card.rs:181-194` `proof_row_renders_count_evidence_and_the_explicit_empty_state`
- `tests/subject_lens_overlay_served_page.rs:777-790` `a_neighborhood_rationale_badge_click_expands_and_does_not_reseed`
- `tests/subject_lens_overlay_served_page.rs:797-810` `the_drill_view_is_byte_identical_with_the_overlay_off`

#### `dup-0473` (near, 8 sites)

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

#### `dup-0474` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs, tests/files_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:83-89` `plain`
- `tests/concepts_lens_view_periphery.rs:108-114` `plain`
- `tests/files_lens_view_periphery.rs:91-97` `plain`

#### `dup-0475` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:120-146` `lens_graph`
- `tests/concepts_lens_view_periphery.rs:140-167` `lens_graph`

#### `dup-0476` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:149-153` `code_default`
- `tests/concepts_lens_view_periphery.rs:170-174` `concepts_default`

#### `dup-0477` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:163-196` `lens_from_query_is_a_public_total_selector_that_falls_back_to_files`
- `tests/concepts_lens_view_periphery.rs:185-213` `lens_from_query_is_a_public_total_selector_including_concepts`

#### `dup-0478` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:209-258` `code_lens_overview_buckets_code_entities_by_community_and_excludes_every_other_kind`
- `tests/concepts_lens_view_periphery.rs:226-279` `concepts_lens_overview_buckets_members_by_concept_across_directories_and_excludes_membershipless_nodes`

#### `dup-0479` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:416-437` `code_lens_at_an_underived_grain_carries_the_documented_empty_state_not_an_error`
- `tests/concepts_lens_view_periphery.rs:463-484` `concepts_lens_at_an_underived_grain_carries_the_documented_empty_state_not_an_error`

#### `dup-0480` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:558-578` `served`
- `tests/concepts_lens_view_periphery.rs:538-558` `served`

#### `dup-0481` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/code_lens_view_periphery.rs, tests/concepts_lens_view_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/code_lens_view_periphery.rs:581-585` `served_json`
- `tests/concepts_lens_view_periphery.rs:561-565` `served_json`

#### `dup-0482` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs, tests/projections_stay_local.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:55-67` `project`
- `tests/concepts_derivation_cli.rs:60-72` `project`
- `tests/projections_stay_local.rs:195-206` `server_project`

#### `dup-0483` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:70-75` `rigger_db`
- `tests/concepts_derivation_cli.rs:75-80` `rigger_db`

#### `dup-0484` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:78-87` `communities`
- `tests/concepts_derivation_cli.rs:83-92` `concepts`

#### `dup-0485` (near, 4 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:92-100` `def`
- `tests/community_detection_cli.rs:105-113` `call`
- `tests/concepts_derivation_cli.rs:96-104` `doc`
- `tests/concepts_derivation_cli.rs:109-114` `link`

#### `dup-0486` (semantic, 2 sites)

Proposed home: `one shared `seed_coupling` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_cli.rs:122-159` `seed_coupling`
- `tests/community_fold_periphery.rs:100-110` `seed_coupling`

#### `dup-0487` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:164-183` `community_layer`
- `tests/concepts_derivation_cli.rs:195-214` `concept_layer`

#### `dup-0488` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:186-191` `member_of`
- `tests/concepts_derivation_cli.rs:217-222` `member_of`

#### `dup-0489` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:265-299` `an_empty_project_records_no_community_and_still_succeeds`
- `tests/concepts_derivation_cli.rs:317-351` `an_empty_project_records_no_concept_and_still_succeeds`

#### `dup-0490` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:302-372` `resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain`
- `tests/concepts_derivation_cli.rs:354-424` `resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain`

#### `dup-0491` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:375-411` `a_malformed_resolution_or_unknown_argument_fails_loudly`
- `tests/concepts_derivation_cli.rs:427-463` `a_malformed_resolution_or_unknown_argument_fails_loudly`

#### `dup-0492` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_cli.rs, tests/concepts_derivation_cli.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_cli.rs:414-457` `re_running_a_grain_reproduces_the_byte_identical_live_layer`
- `tests/concepts_derivation_cli.rs:466-508` `re_running_a_grain_reproduces_the_byte_identical_live_layer`

#### `dup-0493` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_detection_pass.rs, tests/community_resolution_knob.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_detection_pass.rs:67-95` `seed`
- `tests/community_resolution_knob.rs:96-123` `seed`

#### `dup-0494` (semantic, 2 sites)

Proposed home: `one shared `community_snapshot` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_detection_pass.rs:100-115` `community_snapshot`
- `tests/community_fold_periphery.rs:80-95` `community_snapshot`

#### `dup-0495` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:69-75` `live_memberships`
- `tests/concepts_fold_periphery.rs:71-77` `live_realizes`

#### `dup-0496` (semantic, 2 sites)

Proposed home: `one shared `live_memberships` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/community_fold_periphery.rs:69-75` `live_memberships`
- `tests/community_resolution_knob.rs:180-189` `live_memberships`

#### `dup-0497` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:230-263` `a_community_assigned_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`
- `tests/concepts_fold_periphery.rs:218-262` `a_concept_derived_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes`

#### `dup-0498` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_fold_periphery.rs, tests/concepts_fold_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_fold_periphery.rs:266-300` `the_community_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`
- `tests/concepts_fold_periphery.rs:265-306` `the_concept_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type`

#### `dup-0499` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/community_resolution_knob.rs, tests/graph_superseded_prune.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/community_resolution_knob.rs:214-223` `live_memberships_of`
- `tests/graph_superseded_prune.rs:59-68` `live_contains`

#### `dup-0500` (near, 2 sites)

Proposed home: `concepts_labels_membership::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_labels_membership.rs:136-178` `label_is_the_most_central_document_by_intent_degree`
- `tests/concepts_labels_membership.rs:181-212` `label_ties_break_to_the_lexicographically_smallest_document`

#### `dup-0501` (near, 2 sites)

Proposed home: `concepts_labels_membership::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/concepts_labels_membership.rs:262-289` `a_documentless_concept_falls_back_to_its_most_central_members_name`
- `tests/concepts_labels_membership.rs:292-317` `a_documentless_concept_with_no_named_member_falls_back_to_the_most_central_members_id`

#### `dup-0502` (near, 13 sites)

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

#### `dup-0503` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/confidence_tier_blast_radius.rs, tests/criteria_delivery_periphery.rs, tests/gate_store_fence_periphery.rs, tests/run_scoping_survives_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/confidence_tier_blast_radius.rs:52-63` `spawn`
- `tests/criteria_delivery_periphery.rs:43-54` `spawn`
- `tests/gate_store_fence_periphery.rs:775-786` `spawn`
- `tests/run_scoping_survives_periphery.rs:64-75` `spawn`

#### `dup-0504` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/confidence_tier_blast_radius.rs, tests/unified_traversal_grounding.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/confidence_tier_blast_radius.rs:110-115` `fold`
- `tests/unified_traversal_grounding.rs:180-185` `fold`

#### `dup-0505` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:37-64` `courier_project_with_commit`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:38-82` `driver_project`

#### `dup-0506` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:68-77` `run_rigger`
- `tests/courier_registry_refresh_periphery.rs:58-67` `run_rigger`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:97-107` `run_rigger`

#### `dup-0507` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_fence_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:79-86` `assert_ok`
- `tests/courier_registry_refresh_fence_periphery.rs:73-80` `assert_ok`
- `tests/courier_registry_refresh_periphery.rs:69-76` `assert_ok`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:109-116` `assert_ok`

#### `dup-0508` (near, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_fence_periphery.rs, tests/courier_registry_refresh_periphery.rs, tests/gate_store_fence_periphery.rs, tests/registry_refresh_driver_courier_convergence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:91-109` `registry_entries`
- `tests/courier_registry_refresh_fence_periphery.rs:53-71` `registry_entries`
- `tests/courier_registry_refresh_periphery.rs:81-99` `registry_entries`
- `tests/gate_store_fence_periphery.rs:253-271` `registry_entries`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:122-140` `registry_entries`

#### `dup-0509` (semantic, 5 sites)

Proposed home: `one shared `registry_entries` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/courier_registry_refresh_boundary_periphery.rs:91-109` `registry_entries`
- `tests/courier_registry_refresh_fence_periphery.rs:53-71` `registry_entries`
- `tests/courier_registry_refresh_periphery.rs:81-99` `registry_entries`
- `tests/gate_store_fence_periphery.rs:253-271` `registry_entries`
- `tests/registry_refresh_driver_courier_convergence_periphery.rs:122-140` `registry_entries`

#### `dup-0510` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_boundary_periphery.rs, tests/courier_registry_refresh_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_boundary_periphery.rs:263-283` `an_ambient_kurrentdb_conn_never_leaks_into_a_boundary_courier`
- `tests/courier_registry_refresh_periphery.rs:308-333` `an_ambient_kurrentdb_conn_never_leaks_into_a_courier_spawned_through_the_shared_helper`

#### `dup-0511` (semantic, 2 sites)

Proposed home: `one shared `courier_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/courier_registry_refresh_fence_periphery.rs:37-48` `courier_project`
- `tests/courier_registry_refresh_periphery.rs:35-52` `courier_project`

#### `dup-0512` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/courier_registry_refresh_periphery.rs, tests/store_content_identity_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/courier_registry_refresh_periphery.rs:35-52` `courier_project`
- `tests/store_content_identity_periphery.rs:1338-1357` `cli_project`

#### `dup-0513` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_cluster_detail_drill.rs, tests/dash_exploration_route_client_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_cluster_detail_drill.rs:107-109` `spoke_id`
- `tests/dash_exploration_route_client_contract.rs:103-105` `spoke`

#### `dup-0514` (near, 6 sites)

Proposed home: `a new shared module (sites span 6 files: tests/dash_decisions_progressive_disclosure.rs, tests/dash_kg_graph_route.rs, tests/dash_whole_projection_reach.rs, tests/proof_lands_on_the_card_periphery.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_decisions_progressive_disclosure.rs:45-105` `try_fetch_served_root_page`
- `tests/dash_kg_graph_route.rs:110-164` `try_fetch_served`
- `tests/dash_whole_projection_reach.rs:254-314` `try_fetch_whole_served`
- `tests/proof_lands_on_the_card_periphery.rs:773-831` `try_fetch_served`
- `tests/rationale_overlay_data.rs:103-154` `try_fetch_served`
- `tests/rationale_overlay_seam.rs:65-117` `try_fetch_served`

#### `dup-0515` (semantic, 2 sites)

Proposed home: `one shared `exploration_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_exploration_route_client_contract.rs:116-146` `exploration_graph`
- `tests/dash_kg_graph_route.rs:1415-1463` `exploration_graph`

#### `dup-0516` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_exploration_route_client_contract.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_exploration_route_client_contract.rs:151-172` `served_body`
- `tests/subject_lens_reprojection_periphery.rs:330-351` `served_json`

#### `dup-0517` (near, 2 sites)

Proposed home: `dash_graph_exploration_fold::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_graph_exploration_fold.rs:37-62` `cluster_key_is_reachable_over_the_public_crate_boundary`
- `tests/dash_graph_exploration_fold.rs:69-146` `cluster_key_honors_the_boundary_edges_of_the_names_a_file_predicate`

#### `dup-0518` (semantic, 2 sites)

Proposed home: `one shared `fixture_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:39-69` `fixture_graph`
- `tests/metadata_card_periphery.rs:69-105` `fixture_graph`

#### `dup-0519` (semantic, 4 sites)

Proposed home: `one shared `try_fetch_served` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:110-164` `try_fetch_served`
- `tests/proof_lands_on_the_card_periphery.rs:773-831` `try_fetch_served`
- `tests/rationale_overlay_data.rs:103-154` `try_fetch_served`
- `tests/rationale_overlay_seam.rs:65-117` `try_fetch_served`

#### `dup-0520` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/dash_kg_graph_route.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:170-179` `fetch_served`
- `tests/rationale_overlay_data.rs:158-167` `fetch_served`
- `tests/rationale_overlay_seam.rs:121-130` `fetch_served`

#### `dup-0521` (semantic, 4 sites)

Proposed home: `one shared `fetch_served` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_kg_graph_route.rs:170-179` `fetch_served`
- `tests/proof_lands_on_the_card_periphery.rs:835-844` `fetch_served`
- `tests/rationale_overlay_data.rs:158-167` `fetch_served`
- `tests/rationale_overlay_seam.rs:121-130` `fetch_served`

#### `dup-0522` (exact, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/dash_kg_graph_route.rs, tests/dash_whole_projection_reach.rs, tests/proof_lands_on_the_card_periphery.rs, tests/rationale_overlay_data.rs, tests/rationale_overlay_seam.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:182-186` `body_of`
- `tests/dash_whole_projection_reach.rs:329-333` `body_of`
- `tests/proof_lands_on_the_card_periphery.rs:848-852` `body_of`
- `tests/rationale_overlay_data.rs:170-174` `body_of`
- `tests/rationale_overlay_seam.rs:135-139` `body_of`

#### `dup-0523` (near, 2 sites)

Proposed home: `dash_kg_graph_route::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_kg_graph_route.rs:264-321` `the_served_root_page_ships_the_kg_panel_and_select_to_seed_wiring`
- `tests/dash_kg_graph_route.rs:740-784` `the_served_root_page_renders_god_nodes_and_the_query_path`

#### `dup-0524` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dash_release_ready.rs, tests/dash_run_tree_spine.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_release_ready.rs:104-112` `connect_with_retry`
- `tests/dash_run_tree_spine.rs:282-290` `connect_with_retry`

#### `dup-0525` (semantic, 2 sites)

Proposed home: `one shared `connect_with_retry` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/dash_release_ready.rs:104-112` `connect_with_retry`
- `tests/dash_run_tree_spine.rs:282-290` `connect_with_retry`

#### `dup-0526` (near, 4 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:541-581` `an_escalated_unit_renders_gates_failed_and_surfaces_at_the_spec_root`
- `tests/dash_run_tree_spine.rs:591-631` `an_escalated_units_gate_failure_is_not_masked_by_a_trailing_passing_gate`
- `tests/dash_run_tree_spine.rs:710-761` `a_regated_green_unit_renders_gates_passed_despite_an_earlier_failed_attempt`
- `tests/dash_run_tree_spine.rs:775-812` `a_review_rejected_unit_whose_gates_passed_renders_gates_passed_and_surfaces_the_reject`

#### `dup-0527` (near, 2 sites)

Proposed home: `dash_run_tree_spine::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dash_run_tree_spine.rs:820-850` `a_pre_gate_unit_whose_implementer_finished_does_not_render_gates_failed`
- `tests/dash_run_tree_spine.rs:861-898` `a_gates_cleared_unit_with_no_recorded_verdict_still_renders_gates_passed`

#### `dup-0528` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/dedup_seeding_periphery.rs, tests/published_content_key_split_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/dedup_seeding_periphery.rs:390-398` `minted`
- `tests/published_content_key_split_periphery.rs:400-408` `minted`

#### `dup-0529` (near, 2 sites)

Proposed home: `design_intent_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/design_intent_events.rs:209-255` `the_public_emit_lowers_every_concept_kind_onto_the_fold_arm_that_matches_it`
- `tests/design_intent_events.rs:413-456` `the_public_link_emit_lowers_every_link_rel_onto_the_fold_arm_that_matches_it`

#### `dup-0530` (near, 2 sites)

Proposed home: `design_intent_events::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/design_intent_events.rs:796-859` `every_recognized_end_user_usage_shape_is_dropped_before_the_fold`
- `tests/design_intent_events.rs:965-1054` `a_design_word_in_a_non_handbook_usage_doc_does_not_leak_the_handbook_content_keep`

#### `dup-0531` (exact, 5 sites)

Proposed home: `a new shared module (sites span 5 files: tests/duplication_catalog_contract_periphery.rs, tests/handbook_grounder_accuracy.rs, tests/prioritized_plan_citation_periphery.rs, tests/responsibility_map_contract_periphery.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/duplication_catalog_contract_periphery.rs:76-78` `repo_root`
- `tests/handbook_grounder_accuracy.rs:33-35` `repo_root`
- `tests/prioritized_plan_citation_periphery.rs:65-67` `repo_root`
- `tests/responsibility_map_contract_periphery.rs:52-54` `repo_root`
- `tests/simplification_audit.rs:1680-1682` `repo_root`

#### `dup-0532` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/duplication_catalog_contract_periphery.rs:80-84` `read_committed_catalog_raw`
- `tests/responsibility_map_contract_periphery.rs:56-60` `read_committed_map_raw`

#### `dup-0533` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/duplication_catalog_contract_periphery.rs:86-95` `deserialize_committed_catalog`
- `tests/responsibility_map_contract_periphery.rs:62-70` `deserialize_committed_map`

#### `dup-0534` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/duplication_catalog_contract_periphery.rs:102-110` `the_committed_duplication_catalog_deserializes_as_a_downstream_consumer_would`
- `tests/responsibility_map_contract_periphery.rs:77-84` `the_committed_responsibility_map_deserializes_as_a_downstream_consumer_would`

#### `dup-0535` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/duplication_catalog_contract_periphery.rs, tests/responsibility_map_contract_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/duplication_catalog_contract_periphery.rs:299-311` `deserializing_then_reserializing_the_committed_catalog_reproduces_the_committed_bytes_exactly`
- `tests/responsibility_map_contract_periphery.rs:225-237` `deserializing_then_reserializing_reproduces_the_committed_bytes_exactly`

#### `dup-0536` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/gate_store_fence_periphery.rs, tests/spawn_target_dir_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gate_store_fence_periphery.rs:488-504` `init_repo_with_head`
- `tests/spawn_target_dir_periphery.rs:55-71` `init_repo_with_head`

#### `dup-0537` (semantic, 2 sites)

Proposed home: `one shared `init_repo_with_head` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/gate_store_fence_periphery.rs:488-504` `init_repo_with_head`
- `tests/spawn_target_dir_periphery.rs:55-71` `init_repo_with_head`

#### `dup-0538` (near, 2 sites)

Proposed home: `gate_store_fence_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gate_store_fence_periphery.rs:507-577` `a_real_fenced_couriers_scratch_store_is_reclaimed_when_the_worktree_is_removed`
- `tests/gate_store_fence_periphery.rs:580-676` `a_real_fenced_couriers_scratch_store_is_reclaimed_for_a_review_worktree_too`

#### `dup-0539` (semantic, 3 sites)

Proposed home: `one shared `gitsemver_available` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/gitsemver_derivation.rs:83-89` `gitsemver_available`
- `tests/gitsemver_worktree_periphery.rs:117-123` `gitsemver_available`
- `tests/validate_behind_the_tree_periphery.rs:139-145` `gitsemver_available`

#### `dup-0540` (exact, 2 sites)

Proposed home: `gitsemver_derivation::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/gitsemver_derivation.rs:92-110` `a_plain_commit_after_a_tag_increments_the_patch`
- `tests/gitsemver_derivation.rs:113-131` `a_feat_commit_after_a_tag_increments_the_minor`

#### `dup-0541` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_collision_body_and_tiebreak.rs, tests/graph_density_spread_floor_and_centring.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_collision_body_and_tiebreak.rs:98-118` `run_driver`
- `tests/graph_density_spread_floor_and_centring.rs:104-124` `run_driver`

#### `dup-0542` (exact, 2 sites)

Proposed home: `graph_collision_body_and_tiebreak::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_collision_body_and_tiebreak.rs:215-236` `the_collision_body_encloses_the_circle_and_its_label`
- `tests/graph_collision_body_and_tiebreak.rs:242-263` `the_separation_pass_resolves_coincident_nodes_deterministically`

#### `dup-0543` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_denoise_content_survives.rs, tests/graph_denoise_target_project.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_denoise_content_survives.rs:35-42` `fold`
- `tests/graph_denoise_target_project.rs:36-43` `fold`

#### `dup-0544` (near, 2 sites)

Proposed home: `graph_denoise_target_project::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_denoise_target_project.rs:46-198` `a_whole_runs_fold_projects_the_content_but_none_of_the_machinery`
- `tests/graph_denoise_target_project.rs:201-268` `the_actor_metadata_on_a_decision_and_finding_never_folds_an_agent_node`

#### `dup-0545` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/graph_density_spread_floor_and_centring.rs, tests/readable_graph_adaptive_labels.rs, tests/readable_graph_density_scaled_spacing.rs, tests/readable_graph_layout_separation.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_density_spread_floor_and_centring.rs:131-138` `the_served_page_wires_the_layered_path_without_the_density_accessors`
- `tests/readable_graph_adaptive_labels.rs:72-98` `the_served_page_ships_the_adaptive_label_declutter`
- `tests/readable_graph_density_scaled_spacing.rs:63-80` `the_served_page_ships_the_density_spread_lever`
- `tests/readable_graph_layout_separation.rs:56-76` `the_served_page_ships_the_collision_separation_pass`

#### `dup-0546` (exact, 3 sites)

Proposed home: `graph_density_spread_floor_and_centring::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_density_spread_floor_and_centring.rs:184-199` `the_spread_factor_floors_at_one_off_the_dense_path`
- `tests/graph_density_spread_floor_and_centring.rs:259-275` `the_bare_four_arg_layout_stays_panel_sized_and_the_accessor_path_grows_past_it`
- `tests/graph_density_spread_floor_and_centring.rs:323-339` `the_enlarged_canvas_is_centred_on_the_panel_middle`

#### `dup-0547` (semantic, 2 sites)

Proposed home: `one shared `apply_governs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_fold_dedup_live_edge.rs:38-46` `apply_governs`
- `tests/graph_rebuild_collapses_dupes.rs:40-48` `apply_governs`

#### `dup-0548` (exact, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs, tests/graph_superseded_prune.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:51-53` `nanos`
- `tests/graph_rebuild_collapses_dupes.rs:53-55` `nanos`
- `tests/graph_superseded_prune.rs:53-55` `nanos`
- `tests/reset_menu_previews_periphery.rs:71-73` `nanos`

#### `dup-0549` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_fold_dedup_live_edge.rs, tests/graph_rebuild_collapses_dupes.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:57-65` `governs`
- `tests/graph_rebuild_collapses_dupes.rs:59-67` `governs`

#### `dup-0550` (near, 2 sites)

Proposed home: `graph_fold_dedup_live_edge::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_fold_dedup_live_edge.rs:68-106` `subgraph_collapses_repeated_governs_to_one_live_edge_keeping_latest_provenance`
- `tests/graph_fold_dedup_live_edge.rs:109-130` `the_collapsed_edge_keeps_the_earliest_fact_time_and_latest_source_regardless_of_arrival_order`

#### `dup-0551` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:61-77` `run_stream_identity`
- `tests/graph_show_staleness.rs:49-65` `run_stream_identity`
- `tests/graph_show_surface.rs:48-64` `run_stream_identity`

#### `dup-0552` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:80-82` `seed_rigger_dir`
- `tests/graph_show_staleness.rs:68-70` `seed_rigger_dir`
- `tests/graph_show_surface.rs:67-69` `seed_rigger_dir`

#### `dup-0553` (semantic, 3 sites)

Proposed home: `one shared `seed_rigger_dir` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:80-82` `seed_rigger_dir`
- `tests/graph_show_staleness.rs:68-70` `seed_rigger_dir`
- `tests/graph_show_surface.rs:67-69` `seed_rigger_dir`

#### `dup-0554` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:88-102` `run_rigger`
- `tests/graph_show_staleness.rs:96-110` `run_rigger`
- `tests/graph_show_surface.rs:75-89` `run_rigger`

#### `dup-0555` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs, tests/graph_show_surface.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:109-124` `seed_def_lang`
- `tests/graph_show_staleness.rs:83-90` `seed_def`
- `tests/graph_show_surface.rs:94-101` `seed_def`

#### `dup-0556` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:132-135` `open_graph`
- `tests/graph_show_staleness.rs:73-76` `open_graph`

#### `dup-0557` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/graph_show_periphery.rs, tests/graph_show_staleness.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/graph_show_periphery.rs:141-150` `body_line_count`
- `tests/graph_show_staleness.rs:116-125` `body_line_count`

#### `dup-0558` (semantic, 2 sites)

Proposed home: `one shared `body_line_count` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:141-150` `body_line_count`
- `tests/graph_show_staleness.rs:116-125` `body_line_count`

#### `dup-0559` (semantic, 2 sites)

Proposed home: `one shared `assert_light_lane_extent_note` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/graph_show_periphery.rs:157-171` `assert_light_lane_extent_note`
- `tests/graph_show_surface.rs:109-118` `assert_light_lane_extent_note`

#### `dup-0560` (near, 13 sites)

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

#### `dup-0561` (semantic, 2 sites)

Proposed home: `one shared `git_toplevel` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/heartbeat_write_read_agree_periphery.rs:68-78` `git_toplevel`
- `tests/reset_derived_live_writer_guard_periphery.rs:57-68` `git_toplevel`

#### `dup-0562` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/heartbeat_write_read_agree_periphery.rs, tests/reset_derived_live_writer_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/heartbeat_write_read_agree_periphery.rs:128-146` `run_stream_identity`
- `tests/reset_derived_live_writer_guard_periphery.rs:73-87` `run_stream_identity`

#### `dup-0563` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/heartbeat_write_read_agree_periphery.rs, tests/watchdog_cli_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/heartbeat_write_read_agree_periphery.rs:189-194` `now_nanos`
- `tests/watchdog_cli_periphery.rs:208-213` `now_nanos`

#### `dup-0564` (semantic, 2 sites)

Proposed home: `one shared `manifest_text` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:28-32` `manifest_text`
- `tests/turbovec_retired.rs:34-38` `manifest_text`

#### `dup-0565` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:37-52` `table_lines`
- `tests/turbovec_retired.rs:43-58` `table_lines`

#### `dup-0566` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:57-65` `table_declares_key`
- `tests/turbovec_retired.rs:63-71` `table_declares_key`

#### `dup-0567` (semantic, 2 sites)

Proposed home: `one shared `table_declares_key` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:57-65` `table_declares_key`
- `tests/turbovec_retired.rs:63-71` `table_declares_key`

#### `dup-0568` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:164-179` `for_each_rs_file`
- `tests/turbovec_retired.rs:75-90` `for_each_rs_file`

#### `dup-0569` (semantic, 2 sites)

Proposed home: `one shared `for_each_rs_file` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/kurrentdb_always_available.rs:164-179` `for_each_rs_file`
- `tests/turbovec_retired.rs:75-90` `for_each_rs_file`

#### `dup-0570` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/kurrentdb_always_available.rs, tests/turbovec_retired.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/kurrentdb_always_available.rs:190-209` `no_source_still_gates_on_the_retired_kurrentdb_feature`
- `tests/turbovec_retired.rs:168-186` `no_source_still_gates_on_the_retired_turbovec_feature`

#### `dup-0571` (semantic, 5 sites)

Proposed home: `one shared `js_declaration` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 5 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/meta_phases_declaration_periphery.rs:65-87` `js_declaration`
- `tests/phase_of_role_mapping_periphery.rs:50-72` `js_declaration`
- `tests/review_tier_roster_periphery.rs:18-40` `js_declaration`
- `tests/step_attention_periphery.rs:655-677` `js_declaration`
- `tests/worker_persona_label_periphery.rs:21-43` `js_declaration`

#### `dup-0572` (near, 4 sites)

Proposed home: `a new shared module (sites span 4 files: tests/metadata_card_handoff_viz.rs, tests/proof_row_renders_on_the_card.rs, tests/subject_lens_overlay_client_arms.rs, tests/subject_view_memory_rail_client.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/metadata_card_handoff_viz.rs:123-138` `build_harness`
- `tests/proof_row_renders_on_the_card.rs:76-91` `build_harness`
- `tests/subject_lens_overlay_client_arms.rs:82-97` `build_harness`
- `tests/subject_view_memory_rail_client.rs:100-115` `build_harness`

#### `dup-0573` (near, 8 sites)

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

#### `dup-0574` (near, 2 sites)

Proposed home: `migration_is_deliberate_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/migration_is_deliberate_periphery.rs:512-539` `seed_a_retired_entity`
- `tests/migration_is_deliberate_periphery.rs:543-563` `seed_a_live_entity`

#### `dup-0575` (semantic, 4 sites)

Proposed home: `one shared `sigterm_ignorer_in` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/mutation_scratch_reap_base_guard_periphery.rs:54-61` `sigterm_ignorer_in`
- `tests/reap_before_removal_periphery.rs:41-48` `sigterm_ignorer_in`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:138-145` `sigterm_ignorer_in`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:55-62` `sigterm_ignorer_in`

#### `dup-0576` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:65-67` `pkill_word`
- `tests/no_os_kill_audit.rs:71-73` `xkill_word`

#### `dup-0577` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:68-70` `killall_word`
- `tests/no_os_kill_audit.rs:74-76` `pg_signal_word`

#### `dup-0578` (exact, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:80-82` `libc_kill_open`
- `tests/no_os_kill_audit.rs:83-85` `signal_kill_open`

#### `dup-0579` (exact, 4 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:175-177` `shape_pg_signal`
- `tests/no_os_kill_audit.rs:180-182` `shape_libc_kill`
- `tests/no_os_kill_audit.rs:185-187` `shape_signal_kill`
- `tests/no_os_kill_audit.rs:191-193` `shape_direct_rustix_call`

#### `dup-0580` (near, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:196-216` `shape_arg_dashdash`
- `tests/no_os_kill_audit.rs:220-240` `shape_format_dash_brace`

#### `dup-0581` (exact, 2 sites)

Proposed home: `no_os_kill_audit::finding`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:259-268` `fmt`
- `tests/reap_before_removal_audit.rs:131-140` `fmt`

#### `dup-0582` (near, 2 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:273-303` `general_hits`
- `tests/no_os_kill_audit.rs:309-321` `sanctioned_hits`

#### `dup-0583` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs, tests/simplification_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:324-337` `collect_rs_files`
- `tests/reap_before_removal_audit.rs:761-774` `collect_rs_files`
- `tests/simplification_audit.rs:1733-1746` `collect_rs_files`

#### `dup-0584` (semantic, 3 sites)

Proposed home: `one shared `collect_rs_files` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/no_os_kill_audit.rs:324-337` `collect_rs_files`
- `tests/reap_before_removal_audit.rs:761-774` `collect_rs_files`
- `tests/simplification_audit.rs:1733-1746` `collect_rs_files`

#### `dup-0585` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:382-388` `write_file`
- `tests/reap_before_removal_audit.rs:845-851` `write_file`

#### `dup-0586` (near, 11 sites)

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

#### `dup-0587` (near, 4 sites)

Proposed home: `no_os_kill_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:495-503` `arg_dashdash_separator_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:506-514` `negative_pid_format_is_caught_outside_the_sanctioned_files`
- `tests/no_os_kill_audit.rs:571-583` `a_dashdash_separator_inside_a_sanctioned_file_is_still_caught`
- `tests/no_os_kill_audit.rs:586-598` `a_negative_pid_format_inside_a_sanctioned_file_is_still_caught`

#### `dup-0588` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/no_os_kill_audit.rs, tests/reap_before_removal_audit.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_audit.rs:622-635` `the_real_tree_carries_no_forbidden_pattern`
- `tests/reap_before_removal_audit.rs:1725-1738` `the_real_tree_carries_no_bare_removal`

#### `dup-0589` (exact, 4 sites)

Proposed home: `no_os_kill_test_helper_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_test_helper_periphery.rs:176-180` `terminate_pid_refuses_pid_zero`
- `tests/no_os_kill_test_helper_periphery.rs:184-194` `terminate_pid_refuses_pid_one`
- `tests/no_os_kill_test_helper_periphery.rs:213-218` `stop_pid_refuses_pid_zero`
- `tests/no_os_kill_test_helper_periphery.rs:222-229` `stop_pid_refuses_pid_one`

#### `dup-0590` (exact, 2 sites)

Proposed home: `no_os_kill_test_helper_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/no_os_kill_test_helper_periphery.rs:198-200` `terminate_pid_refuses_its_callers_own_pid`
- `tests/no_os_kill_test_helper_periphery.rs:233-235` `stop_pid_refuses_its_callers_own_pid`

#### `dup-0591` (near, 2 sites)

Proposed home: `parallel_ordered_emit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/parallel_ordered_emit.rs:71-77` `drive_default`
- `tests/parallel_ordered_emit.rs:80-89` `drive_paced`

#### `dup-0592` (near, 3 sites)

Proposed home: `phase_of_role_mapping_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/phase_of_role_mapping_periphery.rs:123-138` `plan_and_plan_critique_resolve_to_plan_regardless_of_role`
- `tests/phase_of_role_mapping_periphery.rs:148-163` `review_tier_roles_resolve_to_review`
- `tests/phase_of_role_mapping_periphery.rs:172-187` `implementer_and_any_unrecognized_role_resolve_to_the_fail_visible_build_default`

#### `dup-0593` (semantic, 2 sites)

Proposed home: `one shared `production_main_rs` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:38-48` `production_main_rs`
- `tests/store_resolution.rs:28-32` `production_main_rs`

#### `dup-0594` (semantic, 2 sites)

Proposed home: `one shared `start_kurrentdb` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/projections_stay_local.rs:161-191` `start_kurrentdb`
- `tests/store_resolution.rs:176-207` `start_kurrentdb`

#### `dup-0595` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/projections_stay_local.rs, tests/store_resolution.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/projections_stay_local.rs:269-346` `progress_against_the_server_keeps_progress_db_local_and_the_log_on_the_server`
- `tests/store_resolution.rs:223-298` `a_courier_in_a_project_configured_for_the_server_resolves_the_server_store`

#### `dup-0596` (near, 3 sites)

Proposed home: `proof_lands_on_the_card_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/proof_lands_on_the_card_periphery.rs:354-405` `cross_file_proof_survives_a_real_reextraction_of_the_defining_file`
- `tests/proof_lands_on_the_card_periphery.rs:642-685` `a_deleted_test_reference_retracts_its_stale_proof_through_the_real_pipeline`
- `tests/proof_lands_on_the_card_periphery.rs:716-762` `a_reference_free_tests_dir_files_first_extraction_creates_nothing_and_leaves_no_residue`

#### `dup-0597` (exact, 15 sites)

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

#### `dup-0598` (near, 11 sites)

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

#### `dup-0599` (exact, 7 sites)

Proposed home: `reap_before_removal_audit::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_audit.rs:969-989` `a_reap_authority_name_in_a_trailing_comment_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1020-1039` `an_exemption_marker_inside_a_non_comment_string_literal_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1071-1090` `a_reap_authority_name_inside_a_non_comment_string_literal_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1099-1118` `a_reap_authority_name_inside_a_single_line_block_comment_never_covers_the_removal`
- `tests/reap_before_removal_audit.rs:1262-1280` `an_arbitrary_comment_is_never_mistaken_for_the_exemption_marker`
- `tests/reap_before_removal_audit.rs:1381-1399` `a_doc_comment_mentioning_the_cfg_test_attribute_in_prose_is_never_mistaken_for_it`
- `tests/reap_before_removal_audit.rs:1581-1600` `a_literal_empty_string_authorized_root_argument_never_covers_the_removal`

#### `dup-0600` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reap_before_removal_periphery.rs, tests/worktree_liveness_fence_periphery.rs, tests/worktree_remove_relocated_scratch_base_guard_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_periphery.rs:62-77` `init_repo`
- `tests/worktree_liveness_fence_periphery.rs:120-135` `init_repo`
- `tests/worktree_remove_relocated_scratch_base_guard_periphery.rs:76-91` `init_repo`

#### `dup-0601` (near, 4 sites)

Proposed home: `reap_before_removal_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reap_before_removal_periphery.rs:80-135` `worktree_remove_reaps_a_process_rooted_in_its_sibling_build_cache_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:138-198` `discard_reaps_a_process_rooted_in_the_review_worktrees_fence_sibling_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:201-261` `worktree_remove_reaps_a_process_rooted_in_the_cache_dirs_own_store_fence_sibling_before_reclaiming_it`
- `tests/reap_before_removal_periphery.rs:264-325` `discard_reaps_a_process_rooted_in_the_review_worktree_itself_before_clearing_it`

#### `dup-0602` (near, 2 sites)

Proposed home: `reminder_dedup_workflow_child_env_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reminder_dedup_workflow_child_env_periphery.rs:83-115` `workflow_stamps_its_own_pid_on_the_spawned_child_with_no_inbound_sentinel`
- `tests/reminder_dedup_workflow_child_env_periphery.rs:124-159` `workflow_still_stamps_a_fresh_own_pid_on_the_child_even_when_its_own_reminder_was_suppressed`

#### `dup-0603` (exact, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:141-151` `new`
- `tests/replan_episode_identity.rs:404-414` `new`

#### `dup-0604` (near, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:176-231` `spawn`
- `tests/replan_episode_identity.rs:418-469` `spawn`

#### `dup-0605` (near, 2 sites)

Proposed home: `replan_episode_identity::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:305-384` `a_replan_after_a_critique_reject_supersedes_the_initial_episodes_unit`
- `tests/replan_episode_identity.rs:483-561` `a_second_replan_supersedes_both_earlier_episodes_units`

#### `dup-0606` (near, 3 sites)

Proposed home: `replan_episode_identity::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/replan_episode_identity.rs:753-838` `a_same_id_refine_survives_its_own_episodes_new_sibling_through_the_real_write_path`
- `tests/replan_episode_identity.rs:854-939` `a_same_id_refine_survives_its_own_episodes_new_sibling_walked_first_through_the_real_write_path`
- `tests/replan_episode_identity.rs:954-1044` `a_same_id_refine_survives_its_own_episodes_genuinely_new_unmatched_sibling_through_the_real_write_path`

#### `dup-0607` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_build_cache_periphery.rs, tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_build_cache_periphery.rs:54-57` `seed_store`
- `tests/reset_menu.rs:81-84` `seed_store`
- `tests/reset_menu_identity_migration_periphery.rs:69-72` `seed_store`

#### `dup-0608` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_compaction.rs:131-157` `rows`
- `tests/reset_derived_compaction_periphery.rs:116-139` `raw_rows`

#### `dup-0609` (semantic, 2 sites)

Proposed home: `one shared `edge_inferred` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_derived_compaction.rs:183-185` `edge_inferred`
- `tests/reset_menu_previews_periphery.rs:88-91` `edge_inferred`

#### `dup-0610` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_derived_compaction.rs, tests/reset_derived_compaction_periphery.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_compaction.rs:193-197` `keyed`
- `tests/reset_derived_compaction_periphery.rs:163-167` `keyed`
- `tests/reset_menu_previews_periphery.rs:75-79` `keyed`

#### `dup-0611` (semantic, 2 sites)

Proposed home: `one shared `path_subject_of` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_derived_compaction_periphery.rs:2108-2123` `path_subject_of`
- `tests/store_content_identity_periphery.rs:233-249` `path_subject_of`

#### `dup-0612` (near, 3 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:158-168` `reset_derived_prunes_when_no_run_has_ever_started`
- `tests/reset_derived_live_writer_guard_periphery.rs:731-741` `reset_force_live_alone_is_refused_as_no_mode`
- `tests/reset_derived_live_writer_guard_periphery.rs:747-762` `the_derived_help_entry_documents_force_live_and_owns_the_risk`

#### `dup-0613` (near, 4 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:174-197` `reset_derived_prunes_when_every_unit_is_terminal_and_every_spawn_is_answered`
- `tests/reset_derived_live_writer_guard_periphery.rs:203-229` `reset_derived_ignores_a_prior_runs_unanswered_spawn_and_non_terminal_unit`
- `tests/reset_derived_live_writer_guard_periphery.rs:645-665` `reset_derived_force_live_compacts_despite_an_in_flight_spawn`
- `tests/reset_derived_live_writer_guard_periphery.rs:693-722` `runs_composed_with_a_refused_derived_still_completes_its_own_prune`

#### `dup-0614` (near, 2 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:333-364` `reset_derived_refuses_a_non_terminal_unit_between_spawn_rounds_and_prunes_nothing`
- `tests/reset_derived_live_writer_guard_periphery.rs:373-402` `reset_derived_refuses_an_in_flight_spawn_naming_its_id_and_prunes_nothing`

#### `dup-0615` (near, 2 sites)

Proposed home: `reset_derived_live_writer_guard_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_derived_live_writer_guard_periphery.rs:531-557` `reset_derived_ignores_a_registration_for_a_different_store`
- `tests/reset_derived_live_writer_guard_periphery.rs:576-616` `reset_derived_never_deletes_a_stale_foreign_registry_entrys_file`

#### `dup-0616` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:100-103` `emit`
- `tests/reset_menu_identity_migration_periphery.rs:88-91` `emit`

#### `dup-0617` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs, tests/reset_menu_previews_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:144-149` `code_entity`
- `tests/reset_menu_identity_migration_periphery.rs:113-118` `code_entity`
- `tests/reset_menu_previews_periphery.rs:81-86` `code_entity`

#### `dup-0618` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/reset_menu.rs, tests/reset_menu_identity_migration_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/reset_menu.rs:153-170` `seed_derived_duplicates`
- `tests/reset_menu_identity_migration_periphery.rs:120-137` `seed_derived_duplicates`

#### `dup-0619` (semantic, 2 sites)

Proposed home: `one shared `seed_derived_duplicates` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/reset_menu.rs:153-170` `seed_derived_duplicates`
- `tests/reset_menu_identity_migration_periphery.rs:120-137` `seed_derived_duplicates`

#### `dup-0620` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/review_tier_roster_periphery.rs, tests/worker_persona_label_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:75-129` `run_worker_label_for_unit_and_reviews`
- `tests/worker_persona_label_periphery.rs:72-112` `run_worker_label_for_unit`

#### `dup-0621` (exact, 2 sites)

Proposed home: `review_tier_roster_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:135-148` `the_adversarys_roster_renders_inside_its_action_phrase`
- `tests/review_tier_roster_periphery.rs:154-167` `the_adjudicators_roster_renders_inside_its_action_phrase`

#### `dup-0622` (exact, 2 sites)

Proposed home: `review_tier_roster_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/review_tier_roster_periphery.rs:211-224` `a_roster_on_a_role_with_no_roster_verb_entry_is_never_rendered`
- `tests/review_tier_roster_periphery.rs:229-242` `a_single_entry_roster_renders_with_no_stray_separator`

#### `dup-0623` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/scratch_workdir_config.rs, tests/store_config.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:30-35` `rigger_dir`
- `tests/store_config.rs:32-37` `rigger_dir`

#### `dup-0624` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/scratch_workdir_config.rs, tests/store_config.rs, tests/store_precedence.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:37-39` `write_workflow`
- `tests/store_config.rs:40-42` `write_workflow`
- `tests/store_precedence.rs:64-66` `write_store_conn`
- `tests/store_precedence.rs:78-80` `write_store_config`

#### `dup-0625` (exact, 2 sites)

Proposed home: `scratch_workdir_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/scratch_workdir_config.rs:53-58` `a_present_workdir_deserializes_exactly`
- `tests/scratch_workdir_config.rs:77-83` `a_workflow_with_no_defaults_block_at_all_reads_as_empty`

#### `dup-0626` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:1475-1479` `map_to_json`
- `tests/simplification_audit.rs:2822-2826` `catalog_to_json`

#### `dup-0627` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:2808-2811` `real_files`
- `tests/simplification_audit.rs:2815-2818` `real_catalog`

#### `dup-0628` (near, 4 sites)

Proposed home: `simplification_audit::support (consolidate these 4 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:3065-3227` `render_section_3`
- `tests/simplification_audit.rs:3237-3329` `render_section_4`
- `tests/simplification_audit.rs:3336-3579` `render_section_5`
- `tests/simplification_audit.rs:3665-4150` `render_section_6`

#### `dup-0629` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:4199-4208` `a_simple_free_function_is_found_with_its_line_span`
- `tests/simplification_audit.rs:4439-4445` `production_functions_before_a_cfg_test_mod_are_not_flagged_test`

#### `dup-0630` (exact, 11 sites)

Proposed home: `simplification_audit::support (consolidate these 11 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:4229-4234` `fnv1a_is_not_mistaken_for_the_fn_keyword`
- `tests/simplification_audit.rs:4241-4246` `a_brace_inside_a_line_comment_is_ignored`
- `tests/simplification_audit.rs:4249-4254` `a_brace_inside_a_block_comment_is_ignored`
- `tests/simplification_audit.rs:4257-4262` `nested_block_comments_are_handled`
- `tests/simplification_audit.rs:4265-4270` `a_brace_inside_a_string_literal_is_ignored`
- `tests/simplification_audit.rs:4273-4278` `a_brace_inside_a_raw_string_with_hashes_is_ignored`
- `tests/simplification_audit.rs:4281-4286` `a_brace_inside_a_byte_string_is_ignored`
- `tests/simplification_audit.rs:4289-4294` `a_brace_char_literal_is_not_mistaken_for_real_braces`
- `tests/simplification_audit.rs:4297-4302` `a_lifetime_is_not_mistaken_for_a_char_literal`
- `tests/simplification_audit.rs:4305-4310` `an_escaped_quote_char_literal_does_not_confuse_the_scanner`
- `tests/simplification_audit.rs:4324-4329` `a_trait_default_method_with_a_body_is_recorded`

#### `dup-0631` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:4353-4359` `a_method_inside_an_impl_block_carries_its_header`
- `tests/simplification_audit.rs:4362-4370` `a_trait_impl_header_keeps_the_trait_for_type_text`
- `tests/simplification_audit.rs:4373-4385` `a_method_inside_an_impl_nested_in_a_cfg_test_mod_is_flagged_test`

#### `dup-0632` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:4410-4416` `a_function_directly_in_a_cfg_test_mod_is_flagged_test`
- `tests/simplification_audit.rs:4419-4428` `a_nested_named_test_submodule_is_still_flagged_test_and_named`

#### `dup-0633` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:4514-4520` `a_method_is_classified_under_its_impl_self_type`
- `tests/simplification_audit.rs:4523-4532` `a_method_on_a_generic_impl_block_strips_the_impls_own_leading_generics`
- `tests/simplification_audit.rs:4563-4568` `a_trait_impl_method_is_classified_under_the_implementing_type_not_the_trait`

#### `dup-0634` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:4683-4689` `replace_section_1_only_touches_section_1_leaving_later_sections_intact`
- `tests/simplification_audit.rs:5682-5689` `replace_section_2_only_touches_section_2_leaving_neighbors_intact`
- `tests/simplification_audit.rs:6065-6082` `replace_section_6_only_touches_that_span_leaving_earlier_sections_intact`

#### `dup-0635` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:4733-4755` `responsibility_map_json_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:5825-5847` `duplication_catalog_json_matches_the_tree_or_is_rewritten`

#### `dup-0636` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:4850-4858` `string_and_raw_string_literals_are_one_lit_token_each`
- `tests/simplification_audit.rs:4870-4878` `number_literals_including_a_fraction_are_lit_tokens`

#### `dup-0637` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5128-5154` `three_near_but_not_identical_functions_cluster_as_near_with_every_site_and_one_home`
- `tests/simplification_audit.rs:5157-5175` `cluster_ids_are_assigned_after_deterministic_sort_and_sites_are_sorted_within_a_cluster`

#### `dup-0638` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5300-5313` `proc_stat_or_status_reader_sweep_finds_a_stat_reader_and_a_status_reader_but_not_an_unrelated_fn`
- `tests/simplification_audit.rs:5560-5577` `bespoke_lexer_sweep_finds_the_named_trio_but_not_an_unrelated_fn`

#### `dup-0639` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5320-5333` `the_dash_reap_proc_stat_pair_the_spec_names_lands_in_one_real_cluster`
- `tests/simplification_audit.rs:5432-5447` `the_two_exploration_graph_fixture_builders_the_adversarial_sample_found_land_in_one_real_cluster`

#### `dup-0640` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5336-5343` `constructs_own_type_literal_matches_self_and_the_named_type_but_not_an_unrelated_call`
- `tests/simplification_audit.rs:5346-5354` `constructs_own_type_literal_matches_shorthand_field_init_too`

#### `dup-0641` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5380-5398` `the_spawn_result_constructor_triple_the_adversarial_sample_found_lands_in_one_real_cluster`
- `tests/simplification_audit.rs:5584-5601` `the_bespoke_lexer_and_canonical_extractor_the_lens_routed_land_in_one_real_cluster`

#### `dup-0642` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5450-5478` `same_named_helper_sweep_excludes_required_trait_impl_methods_across_adapters`
- `tests/simplification_audit.rs:5481-5510` `same_named_helper_sweep_excludes_a_trait_default_method_and_its_override`
- `tests/simplification_audit.rs:5513-5537` `same_named_helper_sweep_still_catches_two_inherent_impls_sharing_a_method_name`

#### `dup-0643` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5693-5698` `replace_section_2_panics_loudly_when_the_heading_is_entirely_absent`
- `tests/simplification_audit.rs:6095-6100` `replace_section_6_panics_loudly_when_the_heading_is_entirely_absent`

#### `dup-0644` (exact, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5705-5709` `sample_indices_is_deterministic_for_a_fixed_seed`
- `tests/simplification_audit.rs:5735-5739` `different_seeds_produce_different_draws`

#### `dup-0645` (near, 3 sites)

Proposed home: `simplification_audit::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5854-5891` `report_section_2_matches_the_tree_or_is_rewritten`
- `tests/simplification_audit.rs:5966-6012` `report_sections_3_through_5_match_the_tree_or_are_rewritten`
- `tests/simplification_audit.rs:6158-6191` `report_section_6_matches_the_tree_or_is_rewritten`

#### `dup-0646` (near, 2 sites)

Proposed home: `simplification_audit::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/simplification_audit.rs:5899-5923` `replace_sections_3_to_5_only_touches_that_span_leaving_neighbors_intact`
- `tests/simplification_audit.rs:5926-5943` `replace_sections_3_to_5_falls_back_to_end_of_string_when_no_section_6_heading_exists`

#### `dup-0647` (near, 2 sites)

Proposed home: `spawn_scratch_reap_authorized_root_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spawn_scratch_reap_authorized_root_periphery.rs:167-222` `rigger_result_reaps_a_live_process_in_the_spawns_registered_agent_scratch_dir`
- `tests/spawn_scratch_reap_authorized_root_periphery.rs:225-273` `rigger_result_reaps_a_live_process_in_the_spawns_registered_mutation_scratch_dir`

#### `dup-0648` (near, 2 sites)

Proposed home: `spawn_timing_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/spawn_timing_periphery.rs:104-183` `spawn_timing_pairs_real_writer_events_through_a_real_store_by_role`
- `tests/spawn_timing_periphery.rs:280-335` `spawn_timing_excludes_a_real_same_batch_pair_as_suspect_not_a_silent_zero`

#### `dup-0649` (near, 42 sites)

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
- `tests/spec_lint.rs:1162-1191` `validate_still_flags_an_unsatisfied_either_or_as_an_open_hedge`
- `tests/spec_lint.rs:1207-1238` `validate_still_flags_a_genuine_hedge_after_an_earlier_non_disjunctive_either_on_specs_68_shape`
- `tests/spec_lint.rs:1246-1267` `validate_exempts_the_comma_separated_decided_disposition`
- `tests/spec_lint.rs:1274-1295` `validate_still_flags_a_spaced_negation_before_the_decided_idiom`
- `tests/spec_lint.rs:1302-1323` `validate_flags_a_hedge_split_across_hard_wrapped_lines`
- `tests/spec_lint.rs:1335-1358` `validate_does_not_fuse_a_hedge_across_a_heading_boundary`
- `tests/spec_lint.rs:1365-1388` `validate_does_not_fuse_a_hedge_across_a_table_row_boundary`
- `tests/spec_lint.rs:1402-1429` `validate_ignores_a_double_quoted_span_that_crosses_a_hard_wrapped_line`
- `tests/spec_lint.rs:1439-1465` `validate_ignores_a_backtick_span_that_crosses_a_hard_wrapped_line`
- `tests/spec_lint.rs:1480-1505` `validate_still_flags_a_smell_outside_a_balanced_quote_pair`
- `tests/spec_lint.rs:1513-1538` `validate_still_flags_a_smell_outside_a_balanced_backtick_pair`
- `tests/spec_lint.rs:1548-1575` `validate_fails_closed_after_a_stray_unmatched_quote_earlier_in_the_paragraph`
- `tests/spec_lint.rs:1582-1608` `validate_fails_closed_after_a_stray_unmatched_backtick_earlier_in_the_paragraph`
- `tests/spec_lint.rs:1624-1651` `validate_a_stray_unmatched_quote_does_not_unmask_a_later_real_quoted_disposition_phrase`
- `tests/spec_lint.rs:1671-1697` `validate_ignores_a_backtick_span_whose_closing_mark_is_immediately_after_a_digit`
- `tests/spec_lint.rs:1714-1741` `validate_ignores_a_quoted_span_whose_closing_quote_is_immediately_after_a_digit`
- `tests/spec_lint.rs:1751-1777` `validate_a_digit_adjacent_quote_stays_excluded_as_an_opener`
- `tests/spec_lint.rs:1793-1819` `validate_a_quote_at_the_very_start_of_a_paragraph_is_a_valid_opener`
- `tests/spec_lint.rs:1846-1875` `validate_an_embedded_digit_adjacent_mark_does_not_prematurely_close_a_real_quoted_span`
- `tests/spec_lint.rs:1909-1943` `validate_all_four_digit_adjacency_shapes_together_never_false_positive`
- `tests/spec_lint.rs:1971-1998` `validate_a_digit_glued_to_a_quotes_own_opening_mark_still_masks_the_real_span`

#### `dup-0650` (near, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/step_attention_periphery.rs, tests/workflow_driver_resolved_model_periphery.rs, tests/worktree_liveness_fence_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_attention_periphery.rs:464-484` `temp_git_project_with_commit`
- `tests/workflow_driver_resolved_model_periphery.rs:57-78` `temp_git_project_with_commit`
- `tests/worktree_liveness_fence_periphery.rs:94-115` `temp_git_project_with_commit`

#### `dup-0651` (near, 2 sites)

Proposed home: `step_sheds_the_freshen::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/step_sheds_the_freshen.rs:128-163` `fingerprint_subtree`
- `tests/step_sheds_the_freshen.rs:129-159` `walk`

#### `dup-0652` (near, 2 sites)

Proposed home: `store_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_config.rs:63-75` `a_present_store_block_deserializes_backend_and_url`
- `tests/store_config.rs:92-110` `unrelated_workflow_keys_are_ignored_by_the_lightweight_probe`

#### `dup-0653` (exact, 2 sites)

Proposed home: `store_config::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_config.rs:78-89` `a_workflow_without_a_store_key_reads_as_the_default`
- `tests/store_config.rs:149-160` `an_empty_store_block_and_empty_values_are_no_opinion`

#### `dup-0654` (semantic, 4 sites)

Proposed home: `store_content_identity_periphery::port_double - one canonical constructor, the rest thin variants over it (or a builder), rather than each re-listing every field`

mandatory sweep: parallel constructor functions - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_content_identity_periphery.rs:139-147` `new`
- `tests/store_content_identity_periphery.rs:152-160` `miscounting`
- `tests/store_content_identity_periphery.rs:164-166` `over_an_empty_stream`
- `tests/store_content_identity_periphery.rs:170-178` `over_a_stream`

#### `dup-0655` (exact, 2 sites)

Proposed home: `store_content_identity_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_content_identity_periphery.rs:1507-1509` `detached_subject`
- `tests/store_content_identity_periphery.rs:1519-1521` `mid_character`

#### `dup-0656` (near, 2 sites)

Proposed home: `store_content_identity_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_content_identity_periphery.rs:1511-1513` `past_the_end`
- `tests/store_content_identity_periphery.rs:1515-1517` `inverted`

#### `dup-0657` (semantic, 4 sites)

Proposed home: `one shared `local_event_log` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 4 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_flag_precedence.rs:94-96` `local_event_log`
- `tests/store_precedence.rs:59-61` `local_event_log`
- `tests/store_resolution_cli.rs:66-68` `local_event_log`
- `tests/store_secrets.rs:62-64` `local_event_log`

#### `dup-0658` (near, 4 sites)

Proposed home: `a new shared module (sites span 3 files: tests/store_flag_precedence.rs, tests/store_precedence.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_flag_precedence.rs:125-145` `assert_selected_server`
- `tests/store_precedence.rs:119-139` `assert_selected_server`
- `tests/store_precedence.rs:143-159` `assert_selected_sqlite`
- `tests/store_secrets.rs:106-144` `assert_server_reached_and_credentials_redacted`

#### `dup-0659` (semantic, 2 sites)

Proposed home: `one shared `assert_selected_server` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_flag_precedence.rs:125-145` `assert_selected_server`
- `tests/store_precedence.rs:119-139` `assert_selected_server`

#### `dup-0660` (exact, 2 sites)

Proposed home: `store_flag_precedence::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_flag_precedence.rs:148-165` `run_bare_conn_flag_selects_the_server_never_dropped_to_sqlite`
- `tests/store_flag_precedence.rs:168-183` `run_conn_flag_beats_a_committed_sqlite_store_config`

#### `dup-0661` (semantic, 3 sites)

Proposed home: `one shared `empty_project` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:47-55` `empty_project`
- `tests/store_resolution_cli.rs:54-62` `empty_project`
- `tests/store_secrets.rs:50-58` `empty_project`

#### `dup-0662` (semantic, 2 sites)

Proposed home: `one shared `write_store_conn` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_precedence.rs:64-66` `write_store_conn`
- `tests/store_secrets.rs:70-79` `write_store_conn`

#### `dup-0663` (near, 3 sites)

Proposed home: `store_precedence::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_precedence.rs:217-252` `a_present_but_unreadable_store_conn_surfaces_loudly_not_a_silent_sqlite_fallback`
- `tests/store_precedence.rs:269-312` `an_unknown_committed_backend_surfaces_loudly_not_a_silent_sqlite_fallback`
- `tests/store_precedence.rs:315-355` `a_committed_kurrentdb_backend_with_no_credential_names_all_three_sources`

#### `dup-0664` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/store_resolution_cli.rs, tests/store_secrets.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:78-90` `run_bare_result`
- `tests/store_secrets.rs:88-100` `run_bare_result`

#### `dup-0665` (semantic, 2 sites)

Proposed home: `one shared `run_bare_result` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/store_resolution_cli.rs:78-90` `run_bare_result`
- `tests/store_secrets.rs:88-100` `run_bare_result`

#### `dup-0666` (near, 3 sites)

Proposed home: `store_resolution_cli::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:93-127` `a_server_selected_courier_reaches_the_server_and_never_fabricates_local_sqlite`
- `tests/store_resolution_cli.rs:130-157` `a_courier_with_no_server_configured_resolves_the_local_sqlite_log`
- `tests/store_resolution_cli.rs:160-184` `an_empty_kurrentdb_conn_is_treated_as_unset_not_a_server_with_no_address`

#### `dup-0667` (exact, 2 sites)

Proposed home: `store_resolution_cli::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_resolution_cli.rs:278-285` `prime_resolves_the_configured_server_never_the_local_absent_sentinel`
- `tests/store_resolution_cli.rs:288-297` `stats_resolves_the_configured_server_never_the_local_absent_sentinel`

#### `dup-0668` (exact, 2 sites)

Proposed home: `store_secrets_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/store_secrets_periphery.rs:52-59` `redact_conn_is_a_public_symbol_that_scrubs_userinfo_but_keeps_scheme_host_and_query`
- `tests/store_secrets_periphery.rs:248-256` `redact_conn_scrubs_the_credential_but_keeps_a_benign_at_sign_later_in_the_same_url`

#### `dup-0669` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:60-70` `node`
- `tests/subject_lens_defined_cells_contract.rs:48-58` `node`
- `tests/subject_lens_reprojection_contract.rs:63-73` `node`

#### `dup-0670` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:73-83` `edge`
- `tests/subject_lens_defined_cells_contract.rs:61-71` `edge`
- `tests/subject_lens_reprojection_contract.rs:88-98` `edge`

#### `dup-0671` (exact, 7 sites)

Proposed home: `a new shared module (sites span 4 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs, tests/subject_lens_reprojection_periphery.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:85-87` `code_lens`
- `tests/subject_lens_defined_cells.rs:89-91` `concepts_lens`
- `tests/subject_lens_defined_cells_contract.rs:73-75` `code_lens`
- `tests/subject_lens_defined_cells_contract.rs:77-79` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:101-103` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:106-108` `code_lens`
- `tests/subject_lens_reprojection_periphery.rs:315-317` `code_lens`

#### `dup-0672` (semantic, 3 sites)

Proposed home: `one shared `concepts_lens` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 3 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/subject_lens_defined_cells.rs:89-91` `concepts_lens`
- `tests/subject_lens_defined_cells_contract.rs:77-79` `concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:101-103` `concepts_lens`

#### `dup-0673` (exact, 3 sites)

Proposed home: `a new shared module (sites span 3 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:104-121` `served_json`
- `tests/subject_lens_defined_cells_contract.rs:87-104` `served_json`
- `tests/subject_lens_reprojection_contract.rs:122-142` `served_json`

#### `dup-0674` (near, 3 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:133-149` `communities_no_concepts_graph`
- `tests/subject_lens_defined_cells.rs:255-272` `mixed_membership_graph`
- `tests/subject_lens_reprojection_contract.rs:380-401` `file_over_code_graph`

#### `dup-0675` (exact, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells.rs, tests/subject_lens_defined_cells_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells.rs:510-530` `shared_member_graph`
- `tests/subject_lens_defined_cells_contract.rs:238-256` `shared_member_graph`

#### `dup-0676` (semantic, 2 sites)

Proposed home: `one shared `shared_member_graph` helper (e.g. relocated into `tests/common`) rather than each file defining its own`

mandatory sweep: same-named helper function defined independently in 2+ files - 2 site(s), collected mechanically regardless of the Jaccard pass (spec 85 Design)

- `tests/subject_lens_defined_cells.rs:510-530` `shared_member_graph`
- `tests/subject_lens_defined_cells_contract.rs:238-256` `shared_member_graph`

#### `dup-0677` (near, 2 sites)

Proposed home: `a new shared module (sites span 2 files: tests/subject_lens_defined_cells_contract.rs, tests/subject_lens_reprojection_contract.rs)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_defined_cells_contract.rs:318-334` `full_in_budget_graph`
- `tests/subject_lens_reprojection_contract.rs:154-177` `community_over_concepts_graph`

#### `dup-0678` (near, 3 sites)

Proposed home: `subject_lens_reprojection_contract::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_reprojection_contract.rs:250-284` `reprojection_admits_a_realizing_member_of_any_kind_under_the_concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:493-526` `reprojection_excludes_a_non_code_entity_member_entirely_under_the_code_lens`
- `tests/subject_lens_reprojection_contract.rs:543-573` `reprojection_excludes_a_decision_member_even_when_it_carries_a_live_community_membership`

#### `dup-0679` (near, 2 sites)

Proposed home: `subject_lens_reprojection_contract::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/subject_lens_reprojection_contract.rs:297-328` `reprojection_carries_empty_state_when_no_member_realizes_any_concept_under_the_concepts_lens`
- `tests/subject_lens_reprojection_contract.rs:590-621` `reprojection_carries_empty_state_when_the_sole_realizer_is_purity_excluded`

#### `dup-0680` (near, 7 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 7 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:304-339` `a_spawn_prompt_carries_the_unified_traversal_code_neighborhood_not_the_old_structural_stitch`
- `tests/unified_traversal_grounding.rs:357-461` `the_implement_prompt_is_trimmed_to_the_intent_layer_with_a_rigger_peers_pointer`
- `tests/unified_traversal_grounding.rs:475-515` `the_producer_prompt_keeps_the_full_grounding_context_not_the_implement_trim`
- `tests/unified_traversal_grounding.rs:682-786` `the_sdet_author_build_seam_spawn_receives_the_trimmed_implement_slice`
- `tests/unified_traversal_grounding.rs:1225-1355` `a_spawn_prompt_carries_the_design_intent_that_governs_the_touched_files_by_traversal`
- `tests/unified_traversal_grounding.rs:1444-1501` `a_governing_decision_never_leaks_into_the_spawn_prompt_design_intent_section`
- `tests/unified_traversal_grounding.rs:1578-1611` `a_spawn_prompt_with_no_governing_design_intent_renders_no_design_intent_header`

#### `dup-0681` (near, 2 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:1016-1059` `the_code_neighborhood_section_is_budget_capped_with_a_visible_elision_note`
- `tests/unified_traversal_grounding.rs:1078-1119` `the_spawn_prompt_code_neighborhood_elision_note_names_the_honest_graph_around_recovery`

#### `dup-0682` (near, 2 sites)

Proposed home: `unified_traversal_grounding::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/unified_traversal_grounding.rs:1377-1424` `the_design_intent_section_is_budget_capped_and_its_elision_note_names_the_honest_graph_around_recovery`
- `tests/unified_traversal_grounding.rs:1515-1559` `the_spawn_prompt_design_intent_section_renders_the_newest_binding_and_elides_the_oldest`

#### `dup-0683` (exact, 2 sites)

Proposed home: `validate_advisories::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/validate_advisories.rs:299-312` `validate_is_silent_on_log_bloat_when_every_key_is_recorded_once`
- `tests/validate_advisories.rs:315-343` `validate_is_silent_on_log_bloat_when_the_same_key_recurs_only_across_different_covered_types`

#### `dup-0684` (near, 3 sites)

Proposed home: `watchdog_cli_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/watchdog_cli_periphery.rs:132-182` `watch_once_reports_anomalies_through_the_real_compiled_binary_naming_signal_subject_and_response`
- `tests/watchdog_cli_periphery.rs:299-326` `watch_once_reports_a_store_integrity_anomaly_through_the_real_compiled_binary`
- `tests/watchdog_cli_periphery.rs:573-654` `watch_once_reports_the_criterions_own_multi_anomaly_scenario_through_the_real_compiled_binary`

#### `dup-0685` (near, 3 sites)

Proposed home: `watchdog_cli_periphery::support (consolidate these 3 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/watchdog_cli_periphery.rs:382-449` `watch_without_once_streams_and_re_polls_a_live_mutating_store_until_killed`
- `tests/watchdog_cli_periphery.rs:462-545` `watch_streaming_survives_a_transient_store_read_failure_and_recovers`
- `tests/watchdog_cli_periphery.rs:669-771` `watch_streaming_re_alerts_a_reject_recurrence_churn_count_on_each_increment`

#### `dup-0686` (near, 2 sites)

Proposed home: `worker_persona_label_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/worker_persona_label_periphery.rs:241-253` `a_gap_18_respawn_id_still_renders_the_full_persona_led_label`
- `tests/worker_persona_label_periphery.rs:337-348` `internal_whitespace_is_normalized_before_the_sentence_is_cut`

#### `dup-0687` (near, 2 sites)

Proposed home: `workflow_driver_resolved_model_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/workflow_driver_resolved_model_periphery.rs:163-319` `workflow_driven_rigger_result_meta_resolved_model_reaches_the_persisted_green_event`
- `tests/workflow_driver_resolved_model_periphery.rs:334-473` `workflow_driven_rigger_result_with_no_meta_omits_the_resolved_model_key_and_ignores_a_prose_claim`

#### `dup-0688` (near, 2 sites)

Proposed home: `worktree_liveness_fence_periphery::support (consolidate these 2 sites into one function in this file)`

mechanical: normalized-token Jaccard similarity (8-token shingles, threshold 0.72)

- `tests/worktree_liveness_fence_periphery.rs:297-542` `step_worktree_sweep_discriminates_in_flight_hung_and_terminal_spawns_across_real_process_boundaries`
- `tests/worktree_liveness_fence_periphery.rs:579-674` `gc_integrated_branches_removing_evidence_reaches_real_stderr_for_a_still_registered_worktree_it_alone_reclaims`

### Adversarial sample

Recall check (spec 85 THOROUGHNESS): 30 functions drawn by seeded random index (seed `85072026`, `sample_indices` over all 5988 functions scanned in `src/` and `tests/`, excluding `tests/prioritized_plan_citation_periphery.rs` - criterion 4's own citation-guard periphery test, whose function count grows as its citation-drift-guard mechanism hardens round over round; excluding it keeps that unrelated growth from ever reshuffling this already-verified draw), each read by hand - together with its host file's surrounding context, since a duplicate can live anywhere in the file or a sibling file - to judge whether a duplicate exists that the mechanical pass and the five sweeps above did not already catch.

- `src/community.rs:262-264` `is_empty` - caught: `dup-0017`
- `src/community.rs:754-789` `refinement_splits_an_internally_disconnected_community` - no duplicate found by reading
- `src/conductor.rs:2651-2657` `agent_model` - no duplicate found by reading
- `src/conductor.rs:10017-10030` `stale_units_from_log` - no duplicate found by reading
- `src/conductor.rs:11515-11648` `a_same_id_re_emit_updates_the_proposed_unit_in_place` - no duplicate found by reading
- `src/dash.rs:4917-4919` `new` - no duplicate found by reading
- `src/main.rs:685-691` `store_selection` - no duplicate found by reading
- `src/main.rs:4522-4632` `append_review_quality` - no duplicate found by reading
- `src/main.rs:9381-9390` `retired_entities_advisory` - no duplicate found by reading
- `src/main.rs:9989-10037` `reclaim_orphan_scratch` - no duplicate found by reading
- `src/main.rs:10512-10555` `scratch_footprint` - no duplicate found by reading
- `src/main.rs:23866-23888` `parse_watch_args_accepts_once_and_interval_together_in_either_order` - no duplicate found by reading
- `src/registry.rs:481-508` `a_shared_entry_persists_no_credential` - no duplicate found by reading
- `src/worktree.rs:2930-2943` `shared_build_cache_guard_path_is_a_sibling_lock_file_of_the_cache_dir` - no duplicate found by reading
- `tests/build_env_authority_periphery.rs:369-376` `new` - no duplicate found by reading
- `tests/cli.rs:18476-18537` `setup_installs_every_watching_discipline_skill_into_the_consumer_project` - caught: `dup-0440`
- `tests/cli.rs:20995-21017` `release_ready_is_silent_on_status_for_a_spec_defective_run` - caught: `dup-0377`
- `tests/cli.rs:25857-25896` `step_surfaces_the_same_spec_lint_advisory_as_validate_the_in_run_call_site` - caught: `dup-0453`
- `tests/code_entity_test_exclusion_periphery.rs:1367-1409` `an_out_of_line_cfg_test_module_declaration_excludes_its_declared_file_through_the_public_api` - caught: `dup-0462`
- `tests/dash_whole_projection_reach.rs:159-168` `whole_on_a_fresh_projection_is_empty_and_never_errors` - no duplicate found by reading
- `tests/dedup_seeding_periphery.rs:164-188` `the_predicate_is_a_pure_function_of_the_recorded_stream` - no duplicate found by reading
- `tests/design_intent_events.rs:388-409` `a_doc_link_with_an_unrecognized_rel_folds_nothing_and_never_errors` - no duplicate found by reading
- `tests/duplication_catalog_contract_periphery.rs:146-180` `every_deserialized_site_has_a_well_formed_line_span_and_non_empty_file_and_name` - no duplicate found by reading
- `tests/graph_show_surface.rs:94-101` `seed_def` - caught: `dup-0555`
- `tests/simplification_audit.rs:5157-5175` `cluster_ids_are_assigned_after_deterministic_sort_and_sites_are_sorted_within_a_cluster` - caught: `dup-0637`
- `tests/simplification_audit.rs:5966-6012` `report_sections_3_through_5_match_the_tree_or_are_rewritten` - caught: `dup-0645`
- `tests/step_attention_periphery.rs:736-806` `relay_attention_renders_the_real_wire_produced_by_a_real_step_process` - no duplicate found by reading
- `tests/store_secrets_periphery.rs:37-46` `assert_no_credential` - no duplicate found by reading
- `tests/worker_persona_label_periphery.rs:62-64` `run_worker_label` - no duplicate found by reading
- `tests/worker_persona_label_periphery.rs:241-253` `a_gap_18_respawn_id_still_renders_the_full_persona_led_label` - caught: `dup-0686`

Two real recall gaps surfaced this way and were closed by widening the mechanical sweep with a new generalizable detector each - not a one-off citation - so the fix catches every present and future instance of its class, each pinned by a real-tree regression test: `find_proc_stat_or_status_readers` (decision `u85c2-proc-stat-worked-example`) groups every function reading a `/proc/<pid>/stat` or `/proc/<pid>/status` literal, closing the spec's own named worked example - `src/dash.rs:499-507` `process_state` next to `src/reap.rs:190-197` `pid_starttime`, the same job on the same file with a different field/shape, upheld at spec 62's capstone; `find_parallel_constructor_clusters` (decision `u85c2-parallel-constructor-sweep`) groups 2+ non-test functions per `(file, Self type)` that build a `Self { .. }` / `TypeName { .. }` literal, closing `src/spawn.rs`'s `SpawnResult::liveness_fault` reading MISSING from its own `ok`/`failed` cluster even though all three are parallel constructors for one struct. A third worked example, `exploration_graph` (independently defined test-fixture builders in `tests/dash_exploration_route_client_contract.rs` and `tests/dash_kg_graph_route.rs`), was already caught correctly by the plain Jaccard pass with no sweep needed - confirming the mechanical pass itself has real recall, not only the two widened sweeps. Two further real defects, found on review rather than in this draw, were closed the same way: a RECALL gap the architecture lens routed to this criterion by name across two prior review rounds - this file's own bespoke source-text lexer (`scan_file`/`tokenize`) duplicating the codebase's ONE canonical tree-sitter extractor, `src/grounder/symbols/extract.rs::extract` (its own module doc's claim, architecture 5.5.3) - closed by `find_bespoke_lexer_vs_canonical_extractor` (decision `u85c2-bespoke-lexer-sweep`), a fourth generalizable sweep; and a PRECISION defect the adversary found by reading every `same-named helper` cluster against `ScannedFn::enclosing_impl` - `find_same_named_helper_functions` was misclassifying REQUIRED trait-impl methods as coincidental duplication (`subscribe_all`/`subscribe_stream` across the `EventStore` trait's three backend adapters plus a test double, `blast_radius` across the `Grounder` trait's own default method, its override, and a test double) - closed by excluding members whose extracted Self type differs across the group when at least one comes from an actual `" for "` trait impl (decision `u85c2-same-named-helper-trait-impl-precision-fix`), mirroring `find_parallel_constructor_clusters`'s own `(file, Self type)` keying one function away. Round 7 (decision `u85c4-r7-exclude-periphery-file-from-adversarial-population`) excluded this criterion's own citation-guard periphery file from the draw's population (see this subsection's opening paragraph) and redrew the sample; every one of the 19 functions above marked "no duplicate found by reading" was re-read by hand against its host file's surrounding context, exactly as this THOROUGHNESS check requires whenever the draw changes. 18 of the 19 are genuinely not duplicates; `apply` at `src/conductor.rs:29832-29834` is one shape worth naming so it is not mistaken for a miss - a `Projection` test double's own required trait-impl body, the same port-default/adapter-override/test-double shape `find_same_named_helper_functions`'s trait-impl-precision fix (decision `u85c2-same-named-helper-trait-impl-precision-fix`) already excludes from clustering by design, confirmed to still hold for this draw's own instance of it. The 19th is a genuine small duplicate this catalog's `fn`-only scanner (module doc, THE SCANNER) structurally cannot represent as a cluster: `gate_verdict_event` (`src/conductor.rs:29191-29200`) and the `verdict` closure inside `integrating_a_unit_stales_the_intersecting_downstream_units_cached_verdict_not_the_rest` (`src/conductor.rs:30596-30605`) do the identical job - find the recorded `GateVerdict` for a `"<unit>/gate:g#<attempt>"` replay key, panicking with the same message when none exists - differing only in whether the unit segment is the literal `"s"` or a parameter. A `let`-bound closure is not a `fn` item, so no change to this scanner short of teaching it to see closures could catalog this pair as a cluster; named here, prominently, rather than silently, so a later refactor - or a scanner that learns to see closures - does not miss it.


## 3. Boundary Violations

Instrument: for each of the five named ports (`eventstore::EventStore`, `contextgraph::Projection`, `conductor::AgentDriver`, `gate::Runner`, `grounder::Grounder`), grepped every production (pre-`#[cfg(test)]`) call site of that port's known concrete adapter modules from a NON-adapter, NON-composition-root file, and separately grepped every domain-ish file's top-level `use` statements for a direct infrastructure-crate import. `src/main.rs` is exempt from the "reaches a concrete adapter" check: it is the composition root, and wiring concretions together is its designed job.

FOUND, two violations:

Violation 1 (`AgentDriver`): `src/conductor.rs:7091-7096` (`reclaim_terminal_unit_mutation_scratch`, real production code - well above the `#[cfg(test)] mod tests` boundary this audit's own section 1 identified at `src/conductor.rs:10260`) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` directly by concrete module path. Read via `rigger graph --show AgentDriver`: the port `conductor.rs` actually depends on for driving agents is `trait AgentDriver { fn spawn(&self, agent: &AgentDef, prompt: &str, opts: &SpawnOpts, emit: &dyn Fn(&str, Value) -> Result<(), Error>) -> Result<AgentResult, Error>; }` (`src/conductor.rs:1083-1091`) - one method, `spawn`. Neither called function is about driving an agent or replaying a recorded run (the concern `driver::replay` otherwise owns); both are pure, driver-instance-free scratch-lifecycle utilities that happen to live inside that one concrete adapter's module. The port that should have been used: none exists for this concern yet, which is itself the defect - `conductor.rs` (a use-case/orchestration file) should not need to know which concrete `AgentDriver` implementation happens to define its own mutation-scratch cache-home resolution. Fix direction for a follow-up spec: relocate `cache_home_from` and `reclaim_unit_mutation_scratch` out of `driver::replay` into a neutral, adapter-independent module (a `scratch` or `mutation` support module conductor.rs and every driver adapter can depend on alike), so no use-case file reaches into one specific adapter's internals for a concern that adapter does not conceptually own.

Violation 2 (`Grounder`): `src/ingest.rs:187-211` (`walk_batches`, called from production `conductor::RunCtx::ingest_project_batches` at `src/conductor.rs:7903`, itself called from `src/conductor.rs:7894` well above the `10260` `#[cfg(test)]` boundary) calls `crate::grounder::symbols::events::project_batches_paced` directly by concrete module path at line 197 to reuse the `symbols` grounder's already-persisted index for a one-time whole-project ingest walk, then at line 203 - same function, same missing-port defect, not a separate third violation - calls `crate::grounder::design::events::project_batches` directly by concrete module path for the design-doc half of the same walk. These two calls are the two named sites of section 2's own catalogued twin duplicate pair (`dup-0198`: `src/grounder/symbols/events.rs:36-38` and `src/grounder/design/events.rs:90-114`, both named `project_batches`), so this boundary violation and that duplication finding are two symptoms of one root cause - `ingest.rs` naming each concrete grounder submodule because no port exposes either. The `Grounder` port's own methods (`ground`, `reindex`, `blast_radius`, `index_stamp` - its provenance stamp - all at `src/grounder/mod.rs:133-175`) serve real-time per-query grounding of an agent's prompt; none exposes "hand me every indexed file's projected events for a whole-project batch ingest," so `ingest.rs` - itself a domain ingest authority (its own module doc names it "the ONE walk-and-content-key authority"), not an adapter and not the composition root - has no port to depend on for either call and reaches the concrete `symbols` module (197) and the concrete `design` module (203) directly. Same missing-port defect class as violation 1. Fix direction for a follow-up spec: add an ingest-shaped port method (e.g. a `Grounder::project_batches` or a standalone `SymbolProjector` trait) covering both concrete modules, so `ingest.rs` depends on one abstraction instead of either concrete grounder module for its whole-project walk.

Also reaching `grounder::symbols::store::content_hash` from the same two call sites' neighborhood (`src/ingest.rs:230`, `src/canary.rs:213`): DISPOSITIONED as legitimate shared-primitive reuse, not a third violation. `content_hash` (`src/grounder/symbols/store.rs:49`) is documented at its own definition as "the content-identity primitive" the `symbols` grounder's own reindex-freshening gate keys on, and `canary.rs`'s own doc comment (`canary.rs:191`) separately calls it "the crate's ONE stable content-hash primitive", reused there by deliberate author intent rather than adding yet another open-coded FNV-1a copy - a generic hashing utility that happens to live in the `symbols` module, not a grounding operation reached through the port. The broader duplication this primitive is meant to fix (several open-coded FNV-1a copies elsewhere in the crate, per `src/community.rs`'s own comment at line 67) is a separately tracked cross-cutting refactor (`arch-u2i-fnv1a-fourth-parallel-copy`), not this section's concern.

CHECKED AND CLEAN (three of five ports fully clean; the other two, `AgentDriver` and `Grounder`, are this section's two violations above - each search recorded so a clean result is not merely assumed):
- `eventstore::EventStore` concretion reach (`rusqlite::Connection::open` outside `src/eventstore/sqlite.rs` / `src/eventstore/kurrentdb.rs` / `src/contextgraph/sqlite.rs`): two hits in all of `src/`, one a doc-comment mention (`src/main.rs:4332`) and one a deliberate, explicitly-commented test-only raw-connection bypass (`src/main.rs:23860`, inside `#[cfg(test)] mod tests` opened at `src/main.rs:12650`) that reproduces a pre-append-guard corruption shape `Store::append` itself refuses to construct - a documented test technique, not a boundary violation.
- `contextgraph::Projection` concretion reach (`contextgraph::sqlite::*`): checked whole-tree, not only `src/conductor.rs` - every one of `conductor.rs`'s 28 hits sits inside `#[cfg(test)] mod tests` (production `conductor.rs` only ever depends on `dyn Projection`), and the same is true wherever else `contextgraph::sqlite::Projector` is imported (`src/concepts.rs`, `src/dash.rs`, `src/grounder/symbols/events.rs`, `src/grounder/design/events.rs` - every import sits after that file's own `#[cfg(test)]` boundary); `src/community.rs`'s one mention is a doc comment.
- `gate::Runner` concretion reach (`gate::ExecRunner` / `RecordingRunner`): checked whole-tree, not only `src/conductor.rs`. In `conductor.rs`, production depends only on `dyn gate::Runner` (`conductor.rs:1288`); every mention of a concrete runner before that is a doc comment (`conductor.rs:6498,6545,6615,6621,6625`), and the only actual import and use of `ExecRunner` (`conductor.rs:10266` onward) plus the test-only `RecordingRunner` impl (`conductor.rs:28923,29014`) sit inside `#[cfg(test)] mod tests`, well past the `10260` boundary. Every other source-level `ExecRunner` mention in `src/` is either a doc comment (`src/worktree.rs`, `src/config.rs`, `src/budget.rs`, `src/driver/cli.rs`, `src/lib.rs`) or, in `src/driver/replay.rs`, an import and 13 parameter types that all sit inside that file's own `#[cfg(test)] mod tests` too.
- Use cases importing infrastructure: grepped the top-level `use` statements of every domain-ish file this audit's own code neighborhood names (`src/conductor.rs`, `src/blocker.rs`, `src/spec.rs`, `src/watch.rs`, `src/community.rs`) for `rusqlite`, `reqwest`, `tonic`, `tokio`, `kurrentdb` - zero hits anywhere. Empty category.

A second mutation authority for one domain: the one previously-known instance in this codebase (`src/dash.rs` reimplementing `src/reap.rs`'s `/proc` pid scan, spec 85's own Goal example, upheld at spec 62's capstone) is a duplicate READ-only reimplementation, not a bypassed MUTATION path - it is section 2's finding (`u85c2-proc-stat-worked-example`, `find_proc_stat_or_status_readers`), not re-counted here to avoid double-charging one defect to two sections. Checked git as the one other plausible second-authority candidate: every `Command::new("git")` call site in `src/conductor.rs` (23 sites) is at line >= 17297, inside `#[cfg(test)] mod tests` - production `conductor.rs` never shells to git directly. `src/worktree.rs` is the sole git-worktree-mutation authority OUTSIDE the composition root. Inside it, `src/main.rs` (exempt from the port-concretion-reach check above, not from this one) holds two more git-worktree-mutation sites: `reap_then_remove_worktree` (`main.rs:2791-2805`), the sanctioned worktree half of the spec-34/spec-79 orphan-sweep and extensively reviewed across those specs - a deliberate design choice, not a gap; and `materialize_config_at_rev` (`main.rs:5510-5556`), a real, already-known, non-blocking gap (`arch-u13-config-checkout-bypasses-worktree-authority` / `arch-u2r-config-checkout-shells-git` / `arch-u2r2-replayrunner-and-config-checkout-persist-not-introduced`: the `Worktree` API is branch-creating and exposes no detach-at-rev checkout, so this is a gap in that authority rather than a competing abstraction). No second mutation authority found beyond the already-cited, already-catalogued `/proc` case and this already-dispositioned `materialize_config_at_rev` gap.

## 4. Dead and Vestigial Code

FUNCTIONS WITH ZERO CALLERS. Instrument one (name-reference sweep): scoped to the 596 production (`is_test: false`) entries of the committed `docs/audit/responsibility-map.json` (`src/conductor.rs`, `src/main.rs`, `src/dash.rs` - criterion 1's own scanned scope, reused rather than re-scanned, per this criterion's own no-new-generator-code boundary). For each entry, excluded its own doc-comment and signature span (walking upward from its `start_line` over contiguous `///` / `#[...]` / blank lines) then counted the identifier's remaining whole-tree occurrences. Result: zero functions have zero external references; the lowest tier found is a single real caller (e.g. `src/conductor.rs:316-318` `postmerge_gate_verdict_key`). Instrument two (the knowledge graph, per spec 85's own instruction that it is the cross-checking instrument for this section): `rigger graph --show` resolves a qualified entity and reports a non-zero degree for every one of these low-tier candidates (e.g. `postmerge_gate_verdict_key` reports degree 5), corroborating that a truly isolated function would show degree 0 - had a zero-external-reference candidate existed, the graph would be the confirming check; none did, so the list is empty and the cross-check is vacuously satisfied. A full production-scale caller-list query via `rigger graph --around` on a single small function was tried and found impractical at this scale (a depth-2 traversal pulls in thousands of unrelated GOVERNS-edge decision/finding nodes about the host file, not a clean call list) - disclosed as an instrument limitation rather than silently worked around. Instrument three (the compiler): forced a full library-plus-binary rebuild (touched `src/lib.rs`, `src/conductor.rs`, `src/dash.rs`, `src/main.rs`) on BOTH feature lanes and read rustc's own output - zero warnings on either lane, meaning the default-warn `dead_code` lint (independently enforced further by every unit's own `cargo clippy --all-targets -- -D warnings` gate) finds nothing among the 596 scanned entries. This instrument's reach is narrower than a whole-crate guarantee, though, and this report says so rather than overclaiming: `rigger` is both a library (`src/lib.rs`) and a binary crate, and rustc's `dead_code` lint structurally never fires on a `pub` item in that shape, regardless of its real caller count - a `pub fn` with zero actual callers anywhere compiles and lints exactly as cleanly as one with a hundred, because the lint treats every `pub` item as part of the library's external surface. 399 `pub fn`s exist in `src/` outside the three files instrument one scans (`src/conductor.rs`, `src/main.rs`, `src/dash.rs`), counted over the WHOLE `src/` tree rather than only its top level: 298 in the other 30 top-level `src/*.rs` files, plus 101 more across the 23 files under `src/eventstore/`, `src/grounder/` (its `design/` and `symbols/` submodules included), `src/contextgraph/` and `src/driver/`. Whole-tree is the right scope for this claim: the same structural dead_code-lint blindness argument applies just as much to a `pub fn` in `src/grounder/symbols/store.rs` as to one in a top-level file. None of these 399 - at either level - can instrument three (or instrument one, scoped to those three files, or instrument two, which can only cross-check a candidate the other two already named) structurally rule dead. So this section's zero-dead-code result is proven for the 596 scanned entries, not promised for the whole crate. Exactly one `#[allow(dead_code)]` exists anywhere in `src/` (`src/main.rs:60`, on `mod gitsemver;`); its own preceding comment explains why: the module is shared with `build.rs`, and not every item in it is called from the `main.rs` side - a justified allow, not a live finding.

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

- `page_script` - a small JS snippet fixture - independently redefined in 19 different files (`dup-0342`, exact; e.g. `tests/adaptive_labels_periphery.rs:52-61`, `tests/code_lens_overview_collapse_viz.rs:26-35`, `tests/concepts_lens_view_periphery.rs:689-698`, + 16 more), all inside the Dashboard/viz subsystem (5.1) - cross-validates that grouping.
- `node_available` - a viz-fixture predicate - independently redefined in 19 files; the mechanical pass also clusters it together with the `gitsemver_available`/ `npm_available` availability-check helpers (5 more sites across `src/main.rs` and three test files) into one 24-site cluster (`dup-0240`, exact).
- `temp_project` - a scratch-project-directory fixture - independently redefined in 19 files (`dup-0368`, semantic; e.g. `tests/canary_model_drift_periphery.rs:39-46`, `tests/cause_wire_periphery.rs:54-61`, `tests/cli.rs:19-29`), plus a near-identical 12-site variant (`dup-0367`) and a 13-site `run_rigger` companion helper that drives it (`dup-0369`).
- `run_stream_identity` - a store-identity fixture - independently redefined in 18 files (`dup-0374`, semantic).

Proposed home for all four: `tests/common` (the catalog's own `proposed_home` field already says so verbatim for each). Consolidating just these four collapses roughly 72 duplicate definitions into 4 shared ones - the single largest mechanical simplification this audit identifies anywhere in the test suite.

### 5.3 `tests/cli.rs` split plan

27,074 lines, 351 `#[test]` functions, 15 pre-existing internal section markers in the file: 5 full box-style banner-comment pairs (`tests/cli.rs:11256/11258`, `11816/11818`, `11914/11916`, `12372/12374`, `20514/20527`) plus 10 single-line `// --- Spec NN, criterion M` headers (`21054`, `21972`, `22066`, `23403`, `25425`, `25548`, `25632`, `25770`, `25983`, `26418`). So the file carries some existing, ad hoc organization - each single-line header names the spec and criterion whose tests follow it, not a CLI subcommand or subsystem - rather than the "genuinely flat, not internally organized" state a first read might suggest; 15 markers spread across 351 tests still fall well short of a deliberate, complete per-surface structure. This correction does not disturb the split proposed below: it replaces the file's existing ad hoc, by-spec markers with a complete, deliberate BY CLI SUBCOMMAND SURFACE organization instead. A keyword-on-test-name pass (matching each test's dominant CLI verb: `step_`, `run_`, `validate_`, `reset_`, `watch_`/`watchdog_`, `canary_`, `dash_`/`status_`, `store_`/`eventstore_`, `spawn_`/`mutation_scratch_`/`scratch_`, `review_`/`gate_`, `setup_`/`precommit_`/`hook_`, `courier_`/`registry_`, `spec_`, `replay_`, `worktree_`, `emit_`/`peers_`/`decision_`, `stats_`, `heartbeat_`/`liveness_`, `prime_`/`version_`/`init_`) only cleanly covers 274 of the 351 tests (78%) - disclosed honestly rather than overclaimed, because a real fraction of `cli.rs`'s scenarios are DELIBERATELY end-to-end (a single test legitimately drives `step` + `run` + `validate` + `dash` together to prove a cross-cutting property, e.g. `a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status` or `docs_ships_graph_hygiene_guidance_to_consumers`), which a bare keyword match cannot and should not force into one bucket. The proposed split is BY CLI SUBCOMMAND SURFACE - `cli.rs`'s own natural organizing concept, since the whole file drives the `rigger` binary end to end - into per-surface files (`tests/cli_step.rs`, `tests/cli_run.rs`, `tests/cli_validate.rs`, `tests/cli_reset.rs`, `tests/cli_watch.rs`, `tests/cli_canary.rs`, `tests/cli_dash.rs`, `tests/cli_store.rs`, `tests/cli_review.rs`, `tests/cli_setup.rs`, plus a residual `tests/cli_misc.rs` for the genuinely cross-cutting scenarios), with each test's home decided by its DOMINANT scenario on a human/AI read, not a mechanical keyword match - the same discipline this audit's own responsibility map applied to unassignable functions (named, never silently forced). Cross-referencing the catalog: `cli.rs` also participates in 29 of the catalog's cross-file test-duplication clusters (the most of any single file), several paired against files that WOULD merge with it under this split (`tests/step_attention_periphery.rs`, paired in 6 clusters; `tests/watchdog_cli_periphery.rs`, paired in 2 clusters) - the split is expected to shrink, not grow, the duplication surface.

### 5.4 Duplicated helpers across test files (beyond 5.2's four headline cases)

181 test-only clusters in the committed catalog have every site as an ordinary (non-`#[test]`) helper function - the shared-fixture-extraction candidate class. Beyond the four in 5.2, the widest are: `dup-0346` (`architecture_text` / `eventstore_source` / `main_rs_source` - source-text-loading helpers for doc/architecture-integrity checks, 12 files, 15 sites); `dup-0373` (a companion, 16-file/16-site variant of 5.2's `run_stream_identity` fixture, alongside `dup-0374`'s 18-file version); `dup-0402` (`write_two_stage_workflow` / `write_budget_one_two_stage_workflow` / `write_standalone_review_workflow` - workflow-YAML-literal builders duplicated across `tests/cli.rs` and `tests/step_attention_periphery.rs`, 4 files, 15 sites); `dup-0373`/`dup-0374` (`seed_run_events`, an event-seeding helper, 6-8 files); `dup-0466` (`apply_def_json` / `apply_ref_fresh`-shaped fold-application helpers, 5 files); `dup-0471` (`community` / `concept` / `def`-named single-field constructor helpers, 6 files); `dup-0670` (`code_lens` / `concepts_lens` two-line accessor helpers, 3 files). Every one of these 181 clusters, with its full site list and the catalog's own `proposed_home`, is already machine-readable in the committed `docs/audit/duplication-catalog.json` for a follow-up consolidation spec to consume directly - not re-enumerated exhaustively here to keep this section a report, not a second copy of the catalog.

### 5.5 Table-driven test families

159 test-only clusters have every site as a `#[test]` function - a literal-differs-only-in-input family, spec 85's own named table-driven-test candidate class. The single largest anywhere in the suite: `dup-0649` (near, 42 sites, all in `tests/spec_lint.rs`, e.g. `validate_spec_reports_every_c3_defect_with_its_criterion_and_field_guide_class:54-102`, `validate_spec_attributes_a_prose_level_defect_to_no_criterion:120-163`, `validate_spec_reports_two_simultaneous_defects_on_the_same_criterion:172-209` - 42 near-identical "feed one spec fixture through `validate`, assert one expected defect/advisory line" bodies). Proposed table: `#[test] fn validate_spec_field_guide_defects() { for (fixture, expected) in CASES { ... } }` retiring all 42 named tests into one parametrized loop over a `(&str, &str)` (or richer struct) case table. Other large families: `dup-0597`/`dup-0599` (15+7 sites, `tests/reap_before_removal_audit.rs`, "one fixture function body, one exemption-coverage shape, assert covered/not-covered" - retires into one table keyed by exemption shape); `dup-0630` (11 sites, `tests/simplification_audit.rs` - this very unit's own scanner tests, a `(source, expected_tokens_or_clusters)` table candidate); `dup-0586`/`dup-0587` (11+4 sites, `tests/no_os_kill_audit.rs`, one process-termination-pattern-string per test - a `(pattern, is_caught)` table); `dup-0589` (4 sites, `tests/no_os_kill_test_helper_periphery.rs`, `terminate_pid_refuses_pid_zero` / `_pid_one` x `stop_pid_refuses_pid_zero` / `_pid_one` - a 2x2 `(helper, pid)` table). As with 5.4, the full 157-family list lives in the committed catalog by cluster id for a follow-up test-consolidation spec to consume directly.

## 6. Prioritized Plan

Nineteen follow-up refactoring specs, ordered largest risk-reduction first, plus one category with an explicit no-follow-up-needed disposition (dead and vestigial code, section 4). This section adds no new findings: every citation below points at a claim already recorded in section 1 (`docs/audit/responsibility-map.json`), section 2 (`docs/audit/duplication-catalog.json`), or sections 3-5's own prose. Two instruments ground every count below: the two committed JSON files (queried directly, never re-scanned) and, where a god file's own `#[cfg(test)] mod tests` boundary line is cited, a direct read of that file - the boundary line itself is not a scanner output, it is where in the file the earliest `is_test: true` entry begins. Six of these nineteen entries split a god file (tiers 2 and 3, two phases times three files); the other thirteen retire duplication or close a port gap (tiers 1, 4 and 5) - kept as separate entries throughout, per spec 85's own instruction that "the god-file splits and the duplication removals are separate entries so each can be its own run."

### 6.1 How this plan is ordered

Largest risk-reduction first is read as six tiers, ranked by the KIND of risk each entry retires, highest first:

1. Tier 1 - active correctness risk: a use case already depends on the wrong concretion, or two independent implementations of one concern can already drift apart silently (section 3's two boundary violations; the one already-drifted `/proc`-reading pair section 2 and section 3 both name). These are live gaps, not just size.
2. Tier 2 - god-file test-module extraction: each of the three god files' own inline `#[cfg(test)] mod tests` is the majority of that file's bulk (56-70% by boundary-line count, per a direct read of each file), and moving it is a pure relocation with no production-behavior change - the single largest safe line-count reduction in this plan, and the precondition that makes tier 3 tractable.
3. Tier 3 - god-file production splits: section 1's own proposed module tree applied to the (now much smaller) remaining production surface of each god file. Higher execution risk than tier 2 because it touches live orchestration and CLI logic, so it is sequenced after tier 2 shrinks the target first.
4. Tier 4 - named production duplication sweeps: the mechanical mandatory sweeps section 2 ran regardless of the Jaccard pass (`Command::new`, `.rigger`-path literals, sqlite `Connection::open`, error-shaping helpers), each already a single committed cluster with its own proposed home.
5. Tier 5 - test-suite consolidation: section 5's own catalogued test-only duplication. No production-correctness exposure at all (worst case a test regresses, never the product), so it is ordered ahead only of tier 6 despite touching the largest raw line count anywhere in this plan.
6. Tier 6 - remaining catalog sweep: the 327 src-touching clusters section 2 found but tiers 1 and 4 did not individually name. Unlike every other tier, none of these 327 have been read and risk-assessed one at a time the way tiers 1-4's named clusters have - they are consumed straight from the catalog - so this tier carries production-correctness exposure tiers 2, 3 and 5 do not, and is ordered last: the follow-up spec must triage each cluster's own production-or-test status before merging it, not assume tier 5's blanket test-only treatment applies here too.

Within a tier, entries are ordered largest-first by the site or line count each retires - the same rule the tiers themselves follow, applied one level down.

### 6.2 Tier 1: active correctness risk

#### 1. Close the `AgentDriver` port gap around mutation-scratch reclaim

- Scope: `conductor.rs`'s production `reclaim_terminal_unit_mutation_scratch` (`src/conductor.rs:7091-7096`, section 3 violation 1) calls `crate::driver::replay::cache_home_from` and `crate::driver::replay::reclaim_unit_mutation_scratch` by concrete module path - two pure, driver-instance-free scratch-lifecycle utilities that do not conceptually belong to the `driver::replay` concern they currently live inside. Relocate both into a neutral module every `AgentDriver` adapter and `conductor.rs` can depend on alike (no new trait needed - neither function takes a driver instance, so this is a home fix, not a port-method fix).
- Files: `src/conductor.rs`, `src/driver/replay.rs`, a new home for the two relocated functions.
- Expected line delta: near zero net - a pure move of two functions.
- Risk: low-medium. The reclaim path is covered by spec 83's worktree-lifetime-fenced-by-spawn-liveness contract tests; those tests move with the functions, not get rewritten.
- Unblocks: retires the only `AgentDriver` port violation section 3 found.

#### 2. Close the `Grounder` port gap for whole-project batch ingest (retires dup-0201 in the same motion)

- Scope: section 3 violation 2 (`src/ingest.rs:187-211` `walk_batches`, reaching `grounder::symbols::events::project_batches_paced` and `grounder::design::events::project_batches` by concrete module path) and duplication cluster `dup-0201` (the same two modules' own twin `project_batches` functions, `src/grounder/symbols/events.rs:36-38` / `src/grounder/design/events.rs:90-114`) are one root cause, not two - fix once. TWO CANDIDATES, ONE HOME (spec 85 CONSTRAINTS WALK): `dup-0201`'s own mechanical `proposed_home` suggests relocating into `tests/common`, but both sites are production code under `src/grounder/`, not test helpers - the mechanical heuristic has no "add a port method" category to route a production duplicate to, so it mis-fires here. This plan follows section 3's own reasoned disposition instead: add a `Grounder::project_batches` port method (or a standalone `SymbolProjector` trait) covering both concrete modules, and point `ingest.rs` at it.
- Files: `src/ingest.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/events.rs`, `src/grounder/design/events.rs`.
- Expected line delta: roughly neutral - one new trait method plus two thin impls, minus the two duplicate bodies `dup-0201` catalogs.
- Risk: medium. `ingest.rs`'s own module doc calls it "the ONE walk-and-content-key authority" - a load-bearing path; needs the existing whole-project-ingest and reindex-freshening coverage to stay green, not just the two duplicate-site tests.
- Unblocks: retires the one `Grounder` port violation section 3 found and `dup-0201` together, rather than as two separately-tracked fixes.

#### 3. Retire the duplicate `/proc`-reading authority (`dup-0128` + `dup-0129`)

- Scope: `src/dash.rs::process_state` (`src/dash.rs:499-507`) and `src/main.rs::pgid_of` (`src/main.rs:23064-23077`) each independently re-derive `/proc/<pid>/stat` and `/proc/<pid>/status` fields that `src/reap.rs` (`pid_starttime`/`read_ppid`, `src/reap.rs:190-207`) already parses - the exact "second mutation authority" example spec 85's own Goal names and spec 62's capstone previously caught (`dup-0129`, 14 sites: `src/dash.rs`, `src/main.rs`, `src/reap.rs`, `tests/cli.rs`), plus 59 raw `/proc`-path string literals scattered across `src/dash.rs`, `src/main.rs`, `src/reap.rs` and three test files with no shared composer (`dup-0128`). Both clusters' own `proposed_home` agree: `src/reap.rs` becomes the one `/proc`-reading module; `dash.rs` and `main.rs` call it instead of re-parsing. NOT SYMMETRIC: `process_state` is reachable from `dash`'s own always-on production server, so it is the actual active-correctness risk this tier-1 placement is about; `pgid_of` sits inside `main.rs`'s `mod tests` (opened at `src/main.rs:12650`) and is called only by `#[test]` fns, so on its own it earns no tier-1 placement - it rides in this same item only because it shares `dup-0128`/`dup-0129`'s one root cause and one proposed fix with `process_state`, not because retiring it retires any live risk of its own.
- Files: `src/dash.rs`, `src/main.rs`, `src/reap.rs`, `tests/cli.rs` (`proc_pgid_of`, `tests/cli.rs:23419-23432`, re-points at the same call).
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

#### 10. Consolidate the 655 `.rigger`-path string-literal sites (`dup-0052`) - the single largest cluster in the entire catalog by site count

- Scope: one `.rigger`-relative path-composition helper (the cluster's own `proposed_home`) every one of the 655 sites routes through instead of building its own literal.
- Files: spans dozens of files including `src/conductor.rs`, `src/config.rs`, `src/dash.rs`, `src/docs.rs`, `src/gate.rs`, `src/grounder/mod.rs`, `src/grounder/symbols/store.rs`, `src/ingest.rs`, `src/main.rs`, `src/reap.rs`, `src/registry.rs`, `src/worktree.rs` plus many `tests/` files - the full site list is in the committed `docs/audit/duplication-catalog.json` under `dup-0052` for the follow-up spec to consume directly, not re-enumerated here.
- Expected line delta: negative - 655 literal compositions collapse toward one helper's call sites; the helper itself is small.
- Risk: medium - the largest surface-area sweep in this plan by site count, even though each individual site is trivial; needs a mechanical rewrite pass plus a full-suite green run, not hand-editing 655 sites.
- Unblocks: the biggest single site-count reduction available anywhere in the duplication catalog.

#### 11. Consolidate the 267 `Command::new` call sites (`dup-0006`) behind one injected process-spawn port

- Scope: one process-spawn seam every `Command::new` site routes through (the cluster's own `proposed_home`).
- Files: spans `src/budget.rs`, `src/conductor.rs`, `src/dash.rs`, `src/driver/cli.rs`, `src/gate.rs`, `src/main.rs`, `src/worktree.rs` plus many `tests/` files - full site list in `docs/audit/duplication-catalog.json` under `dup-0006`.
- Expected line delta: negative, though smaller per-site than `dup-0052` since each `Command::new` call already carries real configuration (args, env, cwd) that must move with it, not just a literal.
- Risk: medium-high - several of these 267 sites sit inside `src/budget.rs`'s and `src/conductor.rs`'s already-hardened process-lifecycle code (spec 78's no-os-kill discipline); the follow-up spec must preserve every existing handle-bound-kill invariant at each site it touches, and the no-os-kill gate is the acceptance bar, not merely `cargo test`.
- Unblocks: one seam instead of 267 independent constructions - the next process-spawning concern added anywhere in the crate reuses it instead of adding site 268.

#### 12. Consolidate the 46 sqlite `Connection::open` call sites (`dup-0106`)

- Scope: one sqlite-connection-opening adapter function (the cluster's own `proposed_home`) spanning `src/contextgraph/sqlite.rs`, `src/eventstore/sqlite.rs` and `src/main.rs`, plus several `tests/` files.
- Files: full site list in `docs/audit/duplication-catalog.json` under `dup-0106`.
- Expected line delta: negative - 46 open calls collapse toward one function.
- Risk: medium - touches the event store and context graph's own connection-lifecycle code; needs the store-identity and store-resolution contract tests green throughout.
- Unblocks: one place to change pragma/timeout/journal-mode settings instead of 46.

#### 13. Consolidate the 5 error-shaping helper sites (`dup-0210`) - caution, confirm before merging

- Scope: the cluster spans `src/grounder/mod.rs` (`retired_grounder_error`), `src/worktree.rs` (`revert_on_base_aborts_and_errors_on_a_conflicting_revert`) and three unrelated test files, at line counts from 6 to 78 - a wide spread for one claimed duplicate. This may be a threshold-gaming false cluster (spec 85's own CONSTRAINTS WALK: "the threshold is a floor for the mechanical pass; the reading pass owns semantic duplicates") rather than one real shared concern - the follow-up spec's first job is confirming by reading whether these five sites share actual logic before proposing one helper, not assuming the cluster label proves it.
- Files: `src/grounder/mod.rs`, `src/worktree.rs`, plus the three test files named in `docs/audit/duplication-catalog.json` under `dup-0210`.
- Expected line delta: unknown pending the confirmation read above - potentially zero if the cluster does not survive a human read.
- Risk: low (the smallest-site-count sweep), but with the stated precondition.
- Unblocks: either a genuine fifth consolidation, or a documented "not a real duplicate" disposition that keeps the catalog honest for whoever reads it next.

### 6.6 Tier 5: test-suite consolidation

Every entry cites section 5's own already-catalogued test-only duplication; none of it carries production-correctness risk.

#### 14. Extract the four headline shared test fixtures into `tests/common` (section 5.2)

- Scope: `page_script` (`dup-0342`, 19 files), `node_available` (`dup-0240`, 23 files - merged with two related availability-check helpers), `temp_project` (`dup-0368`, 19 files) and `run_stream_identity` (`dup-0374`, 18 files) - roughly 72 duplicate definitions collapsing into four shared ones, the single largest mechanical simplification section 5 identifies anywhere in the test suite.
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

- Scope, largest first: `dup-0649` (42 sites, `tests/spec_lint.rs`), `dup-0597`/`dup-0599` (15+7 sites, `tests/reap_before_removal_audit.rs`), `dup-0586`/`dup-0587` (11+4 sites, `tests/no_os_kill_audit.rs`), `dup-0630` (11 sites, `tests/simplification_audit.rs` - this very generator's own scanner tests) - 90 sites across 6 clusters.
- Files: the four files named above.
- Expected line delta: negative - each family's near-identical test bodies collapse into one parametrized loop over a table.
- Risk: low - test-only, and each family already shares one body shape (section 5.5's own finding).
- Unblocks: the largest reduction in raw `#[test]` count available in the suite (roughly 90 named tests retiring toward 4).

#### 17. Sweep the remaining 177 test-only helper-duplication clusters (section 5.4, beyond item 14's four headline fixtures)

- Scope: the 181 test-only, all-helper-function clusters section 5.4 names, minus the 4 item 14 already covers - consumed directly from `docs/audit/duplication-catalog.json`, not re-enumerated here (section 5.4's own stated approach). Includes the `dup-0367`/`dup-0369` `temp_project` companion and variant clusters section 5.4 itself places in this "beyond the four" bucket.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative across 177 clusters.
- Risk: low - test-only.
- Unblocks: closes out the helper-duplication half of the test suite's own strict-DRY exposure.

#### 18. Sweep the remaining 153 table-driven test families (section 5.5, beyond item 16's four headline families)

- Scope: the 159 test-only, all-`#[test]` clusters section 5.5 names, minus the 6 cluster ids item 16 already covers - consumed directly from `docs/audit/duplication-catalog.json`. Includes `dup-0587` (4 sites, `tests/no_os_kill_test_helper_periphery.rs`), the smallest of section 5.5's own named large families, left here rather than in item 16.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative.
- Risk: low - test-only.
- Unblocks: closes out the table-driven-test half of the test suite's own strict-DRY exposure; combined with item 17, retires all 340 test-only clusters section 2 found.

### 6.7 Tier 6: remaining catalog sweep

Unlike tier 5, this entry's own clusters are NOT known to be test-only - each one needs its own read before merging (see `### 6.1`'s tier 6 rationale above).

#### 19. Sweep the remaining 327 src-touching duplication clusters (section 2, beyond tiers 1 and 4's 7 named clusters)

- Scope: of the catalog's 674 clusters, 340 are test-only (items 14 and 16-18 above) and 7 are the named tier-1/tier-4 items (`dup-0006`, `dup-0052`, `dup-0106`, `dup-0128`, `dup-0129`, `dup-0201`, `dup-0210`); the remaining 327 clusters touching `src/` - mostly small 2-5-site exact/near matches like the two worked examples section 2 itself opens with (`dup-0001`, `dup-0002`) - are swept here, largest exact-duplicate clusters first, consumed directly from `docs/audit/duplication-catalog.json`.
- Files: per-cluster, from the committed catalog.
- Expected line delta: negative, cumulative; the largest single contributor is whichever exact cluster has the most sites (read from the catalog at spec-writing time, not fixed here).
- Risk: low-medium - unlike tier 5, some of these clusters are production code, so each merge needs its own test-coverage check, not a blanket "test-only" pass.
- Unblocks: the last of the catalog's 674 clusters; after items 1-3 and 10-19 all land, a future spec can state and check that the duplication catalog's own drift guard finds zero live clusters left unaddressed.

### 6.8 Explicitly no follow-up: dead and vestigial code

Section 4 found nothing to remove: zero of the 596 scanned production entries have zero external references (three independent instruments checked - name-reference sweep, the knowledge graph, and a full-rebuild `dead_code` lint on both feature lanes), both named retirements (`turbovec`, `kurrentdb`) are fully clean, and the two stale-looking doc paths found were confirmed generic illustrative examples, not real dangling references. No refactoring spec is proposed for this category (spec 85's own CONSTRAINTS WALK: an empty section states so with the search that established it, never omitted).
