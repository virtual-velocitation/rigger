# 13b - judge-the-judges: canary corpus, model-drift monitor, distilled playbooks

**Goal:** the loop gains the machinery to measure its own review RECALL and to learn across runs - a seeded-defect canary corpus that scores the review panel, a model-drift monitor that triggers a canary when a resolved model id changes, and a post-run distiller that folds lessons into relevance-ranked playbooks. These build on the shipped review panel (lenses -> adversary -> adjudicator), its risk-tiered depth routing, the resolved-model id stamped on each spawn's unit events, and the `LessonLearned` event stream.

## Design

**Unit 1 - seeded-defect canary corpus.** A versioned corpus under `canaries/` (known-good micro-units + cataloged planted defects drawn from the adversary's hunt list) and a `rigger canary` command that runs the review panel against each, scoring: which tier caught the defect, whether the adjudicator's verdict was correct, and verdict stability under finding-order shuffling (position-bias probe). Results land in the event log under a canary namespace; `rigger stats --canary` reports catch rate by tier. This is the loop's only RECALL measurement - everything else measures precision. Owns: judge-the-judges ground truth. Exclusion: automatic scheduling on model change is unit 2's trigger.

**Unit 2 - model-change drift monitor, after unit 1.** When a tier's resolved model id (the id the conductor stamps on each spawn's unit events) differs from the previous run's, `rigger validate` warns and recommends a canary run, and `rigger canary --if-model-changed` (unit 1's canary, gated on that drift) runs only then - the drift monitor for silent alias re-points. Owns: model-drift DETECTION and its canary trigger. Exclusion: the canary corpus and scoring are unit 1's; cross-run lesson distillation is unit 3's - this unit adds no playbooks and touches no prompt injection.

**Unit 3 - distilled playbooks, after unit 1.** A post-run distiller folds the run's `LessonLearned` events into a deduplicated playbook pool (markdown + frontmatter with trigger predicates, rigger's native agent-file shape) stored as derived artifacts under `.rigger/playbooks/` and injected by blast-radius relevance INSIDE the existing per-section prompt budgets (replacing pure recency for the lessons section). The pool is a rebuildable projection of the log - never hand-edited state; `rigger playbooks --rebuild` reconstructs it. Owns: cross-run LESSON distillation and the lessons-section injection ranking. Exclusion: decisions/findings injection (the other budgeted prompt sections) keeps its recency semantics - only the lessons slice gains relevance ranking; model-drift detection and its canary trigger are unit 2's.

## Global constraints

- Hyphens, not em dashes. New event types: NONE - canary scores and distillation ride as metadata on the existing vocabulary; playbooks are derived files, not events. Shipped defaults change NO current behavior (the canary runs only when invoked; the drift monitor warns only on a real model-id change; playbook injection stays within the existing lessons budget). Both lanes green (default features and --no-default-features); replay determinism holds for every new spawn id.

## Done when

- [ ] a canaries corpus and `rigger canary` score per-tier catch rate, adjudicator correctness, and order-shuffle stability into a canary namespace reported by `rigger stats --canary` - pinned with at least three cataloged defect classes
- [ ] a resolved-model change (a tier's stamped model id differing from the previous run's) triggers a `rigger validate` warning and runs `rigger canary --if-model-changed`, while an unchanged model runs no canary - pinned with a seeded model-id change AND a no-change control
- [ ] the post-run distiller folds `LessonLearned` events into a deduplicated, trigger-scoped playbook pool under `.rigger/playbooks/` injected by blast-radius relevance within the existing lessons budget (replacing pure recency for that section), and `rigger playbooks --rebuild` reconstructs the pool from the log - pinned including the rebuild and the relevance-over-recency ordering
