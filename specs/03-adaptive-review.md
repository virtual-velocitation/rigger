# 03 - Adaptive review depth

**Goal:** right-size the three-tier review to the unit's risk - a light review for small, low-risk changes; the full panel for high-risk ones.

## Problem

The three-tier review (lenses -> adversary -> adjudicator) runs identically on every unit. `review_unit` resolves `effective_review_panel(st)` and runs the lenses, the adversary, then the adjudicator regardless of what the unit touched. A one-line change to a leaf file gets the same adversarial gauntlet - and the same spawn cost - as a change to a core trait.

The risk signal already exists and is already computed. `grounded_seed(st)` returns the unit's blast-radius file set (the distinct files the grounder surfaces for the unit's `coverage`/name), the exact set `partition_by_blast_radius` uses to keep concurrent units disjoint and `SpawnOpts.blast_radius` carries to the shim. Nothing consults it to decide review depth.

## Design

Add a review-depth policy to the config and have the conductor select the panel per unit from the unit's grounded blast radius.

- **Config (`config.rs`).** Extend the review configuration so `defaults.review` (and a per-stage `review` override) can carry an optional depth policy. Add a `ReviewDepth` (or fields on a wrapper) holding: a `light` `ReviewPanel` (the reduced roster - typically fewer lenses and/or no adversary), a `threshold` (max blast-radius file count for "low risk"), and a `high_risk_paths` list (path prefixes / globs that force the full panel even for a small change). The existing `ReviewPanel` fields stay the full panel. `ReviewPanel::agent_ids` / the validation in `Config::validate` is extended so the light panel's agent ids are validated too (an unknown light-panel lens fails `config::load` like any other unknown reference).
- **Selection (`conductor.rs`).** Add a `select_review_panel(st, blast_radius) -> &ReviewPanel` (or returns an owned panel) used by `review_unit`. It computes the unit's blast radius via the existing `grounded_seed(st)` and chooses:
  - **full panel** if any blast-radius file matches a `high_risk_paths` entry, OR the blast-radius file count exceeds `threshold`, OR no depth policy is configured;
  - **light panel** otherwise.
  `review_unit` runs the selected panel through the existing tier machinery (`run_agents_concurrently` / `run_adversary` / `run_adjudicator`), so a light panel that omits the adversary simply skips that tier.
- **Backward compatibility.** When no depth policy is configured (the common case and every existing workflow), `select_review_panel` returns the full `effective_review_panel(st)` for every unit - behavior is byte-for-byte unchanged. The policy is opt-in via the new YAML fields.

## Done when

- [ ] `ReviewPanel` / `Defaults.review` gains a depth policy carrying a light panel, a blast-radius threshold, and a high-risk path list, and these parse from the workflow YAML
- [ ] the config validates the light panel's agent ids and the new depth fields, failing `config::load` on an unknown light-panel agent
- [ ] the conductor computes each unit's blast radius and selects the light versus full panel from it before running the three-tier review
- [ ] a low-risk unit (small blast radius, no high-risk path) runs the light panel and a high-risk unit runs the full panel, asserted by a test checking which agents ran for each
- [ ] when no depth policy is configured, every unit runs the full panel exactly as today (behavior unchanged)
