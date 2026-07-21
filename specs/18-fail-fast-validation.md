# 18 - Fail fast: make rigger refuse misconfiguration instead of stalling

**Goal:** close the silent footguns that let a misconfigured persona, spec, or base ref
ferment into an opaque stall. Each fix either refuses up front with a message that names
the fix, or makes an existing silent duplication visible. No new safety mechanism is
added; the loop's bounded escalation is unchanged. This spec implements Workstream A of
`docs/architecture-addendum-pit-of-success.md`.

## Design

Builds on the existing config validation (`config::load`, `ReviewPanel::validate_depth`,
`ReviewPanel::agent_ids` in `src/config.rs`), the `rigger validate` command
(`cmd_validate`, `validate_advisories` in `src/main.rs`), the fail-closed integration
gate (`verdict_approves`, `run_adjudicator`, `IntegrationApproval` in
`src/conductor.rs`), the planner-to-baseline reconciliation (`harvest_proposed`,
`normalize_ws`, `baseline_units`, `PLAN_PROTOCOL` in `src/conductor.rs`), and the run
entry / anchoring (`cmd_workflow`, `parse_run_args`, `load_criteria` in `src/main.rs`;
`Worktree::ensure_run_branch`, `ref_resolves` in `src/worktree.rs`).

**Unit 1 - gating-persona verdict-line static lint (touches `src/config.rs`,
`src/main.rs`).** The integration gate reads a gating agent's RESULT output for a
`{"verdict":...}` line; it never reads emitted events (this is deliberate - see the
addendum's load-bearing decisions). A gating agent (a review adjudicator on any tier, or
a plan-critique adjudicator) whose persona instructs it to record its verdict only via
`rigger_emit`, and never to END ITS OUTPUT with the verdict line, is a guaranteed stall:
the gate finds no verdict, treats it as a non-approval, and the unit remediates until it
escalates. Today validation confirms only that an adjudicator is NAMED. Add a check over
every gating agent's persona prompt: it must instruct the agent to put the verdict on its
output (a `{"verdict"` literal presented as end/output/result/final-line, not exclusively
adjacent to a `rigger_emit` instruction). The check is deterministic, so it is a HARD
error. False negatives (a correct-but-unusual prompt) are acceptable; a false positive (a
prompt that DOES put the verdict on the result but is flagged) is not.

**Unit 2 - run-start refusal on the same defect (touches `src/main.rs`).** The Unit-1
check runs at `config::load` time so a `rigger run`/`rigger workflow`/`rigger step` on a
config with a non-compliant gating persona REFUSES to begin with the same fix message,
rather than starting a doomed run.

**Unit 3 - runtime verdict-channel mismatch detection (touches `src/conductor.rs`).**
Backstops a persona that passed the lint but still returned no verdict. When a gating
spawn returns a result with NO parseable verdict line AND an approve-shaped verdict was
emitted via `rigger_emit` during that spawn, the conductor HARD-ERRORS that unit with the
fix message ("the gate reads the result channel, not emitted events; end your output with
the verdict line"), instead of folding the empty verdict as a reject and remediating.
This is the diagnostic use of events: events explain the failure, the result channel
still decides.

**Unit 4 - spec-shape lint (touches `src/main.rs`, `src/spec.rs`).** `rigger validate`
today accepts no spec argument. Make it accept an optional spec path and emit ADVISORY
warnings (heuristic, never a hard failure) that name the rule and recommend the fix, for:
a checkbox containing multiple observable behaviors; indented sub-bullets under a checkbox
that read as separate criteria; a criterion long enough that a verbatim planner copy is
unreliable. Each advisory recommends "one observable behavior per criterion; put type
shapes and detail in a non-criteria Notes section." Reuses `extract_criteria`
(`src/spec.rs`).

**Unit 5 - planner-to-baseline stable-id match (touches `src/conductor.rs`).** The
conductor reconciles a planner's proposed unit against its baseline by comparing the
criterion text with only whitespace normalization (`normalize_ws`), so a planner that
paraphrases or truncates a criterion it was told to copy verbatim produces a proposal
that does not match, and BOTH run - duplicate ownership, reject loop. Give each baseline
criterion a stable id (its position plus a normalized content hash); have the planner echo
that id (update `PLAN_PROTOCOL`); match on the id. A proposal that maps to no baseline id
still runs as a genuinely-new sub-unit (unchanged behavior for real splits) but the
conductor records a VISIBLE `unmatched-proposal` signal on the existing decision/finding
surface (no new event type) so the extra unit is legible. A proposal mapping to a
baseline id already claimed is merged, never double-run.

**Unit 6 - `--base` reachability + missing-files refusal (touches `src/main.rs`,
`src/worktree.rs`).** The default base `origin/main` is correct and stays, but is
unreachable-to-override on the commands an operator uses: `rigger workflow <spec> --base`
errors "expected at most one spec path" and `rigger run --base` errors "unknown flag".
Accept `--base <ref>` on both `cmd_workflow` and `parse_run_args` and thread it to the run
anchor (the native workflow already threads a base). Separately, before a run parks its
first unit, extract path-like tokens from the spec's criteria (e.g. `crates/foo/src/bar.rs`,
`src/x/y.rs`) and check them against the base ref; if NONE resolve in the base, REFUSE with
a message naming a missing path and suggesting `--base <your-branch>`. Refuse only on total
absence (a strong wrong-base signal); a partial match warns. The default is unchanged.

**Unit 7 - rigger version + actionable build-provenance drift diagnostic (touches
`src/main.rs`, adds a build script for provenance).** An agent cannot today tell whether
the installed `rigger` binary matches the source, so the workflow-drift warning ("the
installed workflow differs from the binary's embedded copy") is ambiguous: it does not say
WHICH side is stale, so resolving it (rebuild the binary vs `rigger setup` to refresh the
workflow) falls back to a human. Make version self-serve: `rigger version` (and `rigger
--version`) reports the crate version AND build provenance - a git commit / describe
identifier embedded at build time - so any agent can identify the exact binary. Use that
provenance in the workflow-drift diagnostic to name WHICH side is stale and give the
directive fix: if the binary's build commit predates the workflow source, "the binary is
stale; rebuild it"; if the installed workflow predates the binary's embedded copy, "run
`rigger setup` to refresh the workflow" - never an ambiguous "they differ". Surface the
version on the agent-visible paths (`rigger setup` output and `rigger validate`).

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- No new event types; the `unmatched-proposal` signal rides the existing decision/finding
  surface.
- Determinism by construction: anything serialized uses `BTreeMap`/`BTreeSet`/sorted
  `Vec`, never `HashMap`/`HashSet`; the criterion-id hash is content-stable and
  line-ending-normalized.
- BOTH feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` - on the default features AND on `--no-default-features`.
- The load-bearing decisions of the addendum are preserved: the gate still reads only the
  result channel; the base default stays `origin/main`; no path is scoped away from
  cross-run grounding.

## Done when

- [ ] a fixture proves `rigger validate` HARD-errors on a config whose gating adjudicator persona never instructs a result-channel verdict line (only `rigger_emit`), with a message naming the fix, and PASSES on an otherwise-identical config whose persona ends its output with the verdict line
- [ ] a fixture proves a run entry (`config::load`) REFUSES to start on that same non-compliant gating persona with the same fix message, and starts on the compliant one
- [ ] a fixture proves a gating spawn that returns no parseable verdict line but emitted an approve-shaped verdict via `rigger_emit` HARD-errors that unit with the result-channel fix message, rather than being folded as a reject and remediated
- [ ] a fixture proves `rigger validate <spec>` emits a NAMED advisory (not a hard failure) for a multi-behavior checkbox, a sub-bullet-as-unit, and an over-long criterion, and emits none for a clean single-behavior spec
- [ ] fixtures prove a PARAPHRASED planner proposal matches its baseline by stable id (exactly one unit runs, no duplicate), a VERBATIM copy still supersedes its baseline, and a genuinely-new proposal runs and records a visible `unmatched-proposal` signal
- [ ] a fixture proves `rigger workflow <spec> --base <ref>` and `rigger run <spec> --base <ref>` both accept and thread `--base` to the run anchor (no "expected at most one spec path" / "unknown flag" error), and the default with no flag is still `origin/main`
- [ ] a fixture proves a run whose spec criteria reference only paths ABSENT from the base ref refuses with a message naming a missing path and suggesting `--base`, and proceeds when the base contains them
- [ ] a test proves `rigger version` and `rigger --version` report the crate version and a build-provenance identifier (a git commit/describe embedded at build time)
- [ ] fixtures prove the workflow-drift diagnostic names WHICH side is stale (installed workflow vs binary) using build provenance and gives the directive fix (rebuild the binary vs `rigger setup`), for BOTH drift directions, rather than an ambiguous "they differ"
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`)
