# 13b - model-drift monitor + distilled playbooks

**Goal:** two cross-run learning features that build on the shipped harness - the seeded-defect canary (`rigger canary`, which runs the review panel against a planted-defect corpus and scores its recall), the resolved-model id the conductor stamps on each spawn's unit events, and the `LessonLearned` event stream: a model-drift monitor that triggers a canary when a resolved model id changes, and a post-run distiller that folds lessons into relevance-ranked playbooks.

## Design

**Unit 1 - model-change drift monitor.** When a tier's resolved model id (the id the conductor stamps on each spawn's unit events) differs from the previous run's, `rigger validate` warns and recommends a canary run, and `rigger canary --if-model-changed` (the shipped canary, gated on that drift) runs only then - the drift monitor for silent alias re-points. Owns: model-drift DETECTION and its canary trigger. Exclusion: the canary corpus and its scoring are the shipped `rigger canary` command's, not re-implemented here; cross-run lesson distillation is unit 2's - this unit adds no playbooks and touches no prompt injection.

**Unit 2 - distilled playbooks.** A post-run distiller folds the run's `LessonLearned` events into a deduplicated playbook pool (markdown + frontmatter with trigger predicates, rigger's native agent-file shape) stored as derived artifacts under `.rigger/playbooks/` and injected by blast-radius relevance INSIDE the existing per-section prompt budgets (replacing pure recency for the lessons section). The pool is a rebuildable projection of the log - never hand-edited state; `rigger playbooks --rebuild` reconstructs it. Owns: cross-run LESSON distillation and the lessons-section injection ranking. Exclusion: decisions/findings injection (the other budgeted prompt sections) keeps its recency semantics - only the lessons slice gains relevance ranking; model-drift detection and its canary trigger are unit 1's.

## Global constraints

- Hyphens, not em dashes. New event types: NONE - drift detection rides the existing resolved-model id stamps; playbooks are derived files, not events. Shipped defaults change NO current behavior (the drift monitor warns only on a real model-id change; playbook injection stays within the existing lessons budget). Both lanes green (default features and --no-default-features); replay determinism holds for every new spawn id.

## Done when

- [ ] a resolved-model change (a tier's stamped model id differing from the previous run's) triggers a `rigger validate` warning and runs `rigger canary --if-model-changed`, while an unchanged model runs no canary - pinned with a seeded model-id change AND a no-change control
- [ ] the post-run distiller folds `LessonLearned` events into a deduplicated, trigger-scoped playbook pool under `.rigger/playbooks/` injected by blast-radius relevance within the existing lessons budget (replacing pure recency for that section), and `rigger playbooks --rebuild` reconstructs the pool from the log - pinned including the rebuild and the relevance-over-recency ordering
