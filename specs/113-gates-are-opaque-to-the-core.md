# 113 - Gates are opaque to the core

**Goal:** rigger's code knows no gate by name, in the sense THE TEMPLATE SETS decides. A gate is a
command a project's `.rigger/workflow.yml` wires into a stage, so adding or removing one there
changes no Rust source and no test text (issue #33), while the three retained tests that load this
repository's workflow through the validating `config_store::load` need, at run time, whatever
executables that workflow's `requires` names, which is the environment, not the test (TESTS PIN
FIXTURES AND THE SCAFFOLD).
Today the core knows one gate and its tool. Lines naming them - `cargo-mutants`, `cargo mutants`,
`mutation.sh`, `rigger-mutants`, the upper-case word `MUTANTS`, `MUTATION_GATE_ID` or the string
literal `"mutation"` - number 130 in 17 files under `src/` and `crates/`
(`crates/rigger-conductor/src/conductor.rs` 25, `src/cli/mod.rs` 20,
`crates/rigger-gates-shell/src/gate.rs` 19, `crates/rigger-driver/src/driver/replay.rs` 17,
`crates/rigger-config-files/src/config_store.rs` 9, `src/cli/setup.rs` 7, `src/cli/validate.rs` 6,
`crates/rigger-process/src/reap.rs` and `crates/rigger-worktree-git/src/worktree.rs` 5 each,
`src/cli/run.rs` 4, `crates/rigger-domain/src/spawn.rs` and
`crates/rigger-grounder/src/grounder/workflowdef.rs` 3 each, `crates/rigger-domain/src/config.rs` and
`crates/rigger-grounder/src/grounder/symbols/extract.rs` 2 each, and one each in
`crates/rigger-dash/src/dash.rs`, `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` and
`src/cli/dashboard.rs`), and 104 in 14 files under `tests/` (`tests/cli.rs` 57). They fall in three
classes.

- Core knowledge to remove. `Config::validate` probes `PATH` for `cargo-mutants` when the workflow
  declares a gate keyed `MUTATION_GATE_ID` (`mutation_gate_binary_on_path`), and
  `build_environment_report` prints a dedicated mutation-gate line. `SCAFFOLD_GATE_FILES` embeds
  `.rigger/gates/mutation.sh` by path, and `SCAFFOLD_WORKFLOW` declares seven gates whose every
  command is `echo <id> ok; true`, so the scaffolded TDD gate cannot fail. `ExecRunner::run` exports
  `MUTANTS`, which `run_gates_at` derives as a `cargo-mutants-<slug>` sibling of the unit worktree
  (`UNIT_MUTANTS_PREFIX`). A spawn-keyed `<cache home>/rigger-mutants` root that no persona, gate
  script or prompt writes is still derived, reaped and measured (`mutation_scratch_path`,
  `reclaim_unit_mutation_scratch`, `mutation_scratch_root`, `reclaim_terminal_unit_mutation_scratch`,
  `reclaim_spawn_registered_scratch`, the `mutation_root` input of `footprint_report`).
  `REVIEWER_DISCIPLINE` forbids reviewers one tool by name.
- Generic mechanisms that stay: the unit-sibling derivation and its reaper (`unit_sibling`,
  `reclaim_cache_sibling`), the per-step orphan backstop (`reclaim_orphan_scratch`), the retired-key
  refusal of `build.mutation`, the `RIGGER_RUN_BASE` export and the built-in instruction prose on
  mutation testing.
- Tests to re-home: tests that read this repository's own `.rigger/workflow.yml` gate list, gate
  commands or stage coverage, such as
  `rigger_workflow_yml_wires_the_checkin_stage_and_mutation_gate_with_the_spec_91_shape`, which
  asserts the live check-in stage lists no `mutation` gate.

## Design

**UNIT ORDER AND BASE, decided here.** Criteria 1, 2, 3 and 7 need nothing. Criterion 4 needs
criterion 3 (it classifies the name criterion 3 introduces). Criterion 5 needs criteria 1, 2 and 3
(its scaffold comment names `requires`, the principle-gate helper it deletes loses its
live-workflow mode in criterion 2, and its handbook sentence that rigger no longer sets `$MUTANTS`
is true only once criterion 3 stops `ExecRunner::run` exporting it). Criterion 6 needs criterion 5.
Criterion 8 needs criteria 1, 3, 4, 5 and 7 (its rule stays red until their removals land).
Criterion 9 needs all eight. The spec is launched on rigger-run. No unit edits this repository's
`.rigger/workflow.yml`, `.rigger/agents/` or `.rigger/instructions/`: they are the run's pinned
definition (`definition_hash`), and the operator's installed binary refuses unknown workflow keys,
so a `requires` key there would stop the run that builds it.

**A GATE DECLARES WHAT IT REQUIRES, decided here.** `config::Gate`
(`crates/rigger-domain/src/config.rs`) gains `requires: Vec<String>` under the config key
`gates.<id>.requires`, `#[serde(default)]`: executable names, empty when absent. Criterion 1 adds
the field to every `config::Gate` struct literal, and a test another criterion adds that builds a
`config::Gate` in Rust does so through `gate_def` or `gate_def_inputs`
(`tests/common/fixtures/config.rs`), so it needs no edit whether it lands before criterion 1 or
after it. An entry means an executable on `PATH`, by decision: a `cargo` subcommand `cargo install`
put in `$CARGO_HOME/bin` resolves only while that directory is on `PATH`, and the Rust set's comment
tells the consumer so (Notes). One resolver in `crates/rigger-gates-shell/src/gate.rs`,
`resolve_requirements(gates, path_var)`, resolves every entry of every declared gate in gate-id
order, then list order, as written (a name listed twice resolves twice), and answers the first
missing entry as its `Err`, `RequirementUnavailable`, so its success value holds resolved entries
only. It looks each entry up as a file name in each `PATH`
directory in order through `find_executable`, which returns the first executable regular file it
finds; `path_has_executable`, the wrapper probe, calls it, so one lookup serves both.
`find_executable` searches only absolute `PATH` directories and skips an empty or relative `PATH`
component, so every resolved `at` path is absolute and the wrapper probe skips the same components.
A named `build.wrapper` reachable only through such a component is therefore refused at load as not
on `PATH`, by the `WrapperUnavailable` refusal `Config::validate` raises through
`resolve_build_layer`, and `build.wrapper: auto` injects nothing when its `KNOWN_WRAPPERS`
candidates are reachable only that way, as when none is installed.
`find_executable` follows symlinks, reading `std::fs::metadata` as `is_executable_file` does today,
never `symlink_metadata`: a symlink to an executable regular file resolves at the symlink's own
path, and a dangling symlink resolves missing. An empty `requires` entry or one containing `/` names
no file in a `PATH` directory and resolves missing. `resolve_requirements_on_path(gates)` is its
ambient-`PATH` edge. `Config::validate` calls that edge last, after every other refusal -
`resolve_build_layer`'s first, then the retired-key refusals, `defaults.review`'s, each stage's,
the cycle check's and, immediately before it, `Workflow::failure_taxonomy`'s - and refuses on its
`Err` with the one message shape (Notes), whether or not a stage lists the gate: a declared gate is
a gate the operator wired. A workflow failing two checks therefore reports the earlier check's
message, and a requirement refusal reaches only a workflow every other check accepts. A `Config`
handed to `conductor::run` without a validating load has no requirement check, by decision - one
resolver, run once per validating load - and a gate whose requirement is missing there fails at its
own command. That differs from `build.wrapper`, which `build_env` re-resolves through
`resolve_build_layer` at run time and refuses there for such a `Config`, because the wrapper
changes the command the run executes, while a requirement only moves a failure earlier. A gate named
`mutation` that declares nothing validates with no
`cargo-mutants` on `PATH`; only `tests/cli.rs` asserts that, and the `config_store` test of a requirement-less gate
uses the id `sweep`. A `match` or `if` on a gate id is NOT an implementation of this check, and
neither is a requirement list kept anywhere but the gate's own `requires`. `MUTATION_BINARY`,
`MUTATION_GATE_ID`, `MutationBinaryUnavailable`, `mutation_gate_binary_available` and
`mutation_gate_binary_on_path` are deleted with their tests, and every comment that describes the
deleted probe describes the requirement check instead (the `config_store` module doc,
`read_scratch_defaults`'s doc, the doc of `BuildConfig::mutation`, the
`crates/rigger-config-files/Cargo.toml` comment). The `build.mutation` refusal keeps its key and
spec number, and its reason becomes `spec 91 retired it: nothing reads it`. `SCAFFOLD_WORKFLOW`
loses the sentence claiming a gate under the id `mutation` requires `cargo-mutants`.
`.github/workflows/rust.yml` stops installing `cargo-mutants`, and that install step's comment
gives only the `sccache` reason.

**VALIDATE REPORTS THE LOAD'S OWN RESOLUTION, decided here.** A load resolves the requirements once:
`Config::validate` returns `resolve_requirements_on_path`'s success value, and
`config_store::load_with_gate_requirements` returns it beside the config as `LoadedConfig`.
`config_store::load` returns that call's config, so its other callers change nothing, and
`rigger validate` is the one reader of the list: it loads through `load_with_gate_requirements` and,
after its `build budget:` line, prints one line per declared gate in gate-id order, rendered by a
pure `gate_requirement_lines` from that list, never from a second resolution:
`gate <id>: requires nothing`, or `gate <id>: requires <name> at <path>` with further entries joined
by `, `. A missing requirement is the resolver's `Err`, never an entry, so the list
`gate_requirement_lines` takes cannot hold an unresolved requirement, and the command refuses before
any output. `build_environment_report` loses its third parameter and its mutation line.

**TESTS PIN FIXTURES AND THE SCAFFOLD, NEVER THE LIVE WORKFLOW, decided here.** No test reads this
repository's `.rigger/workflow.yml` gate list, gate commands or stage coverage; each test that does
is deleted or re-homed as the Notes table LIVE-WORKFLOW TESTS says. A test that reads the live
workflow but skips when a gate is missing is still a live-workflow read. The check-in-once behaviour
is asserted on a fixture through the conductor: a fixture workflow whose fan-out `implement`
template lists only the gate `unit-check` and whose `checkin` stage (`needs: [implement]`) lists
only the gate `sweep`, run through `run_isolated` with a `RecordingRunner` that passes every gate,
once on a two-criterion spec and once on a three-criterion spec. Criterion 2 changes no production
code, and that fixture test is a behaviour pin: today's conductor readies a stage that needs the
fan-out template only once every criterion unit integrates
(`a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates`), so the
test is green when unit 2 starts and pins what the conductor already does in place of the
live-workflow pins the unit deletes, never code written before its test; the unit's TDD evidence is
that pin and the deletions, never a red. A test that reads this repository's workflow for something
other than gates stays, on the loader it uses today:
`shipped_workflows_carry_a_non_zero_spawn_budget` (`src/cli/mod.rs`),
`decomposes_the_real_spec_01_into_per_criterion_units` (`crates/rigger-conductor/src/conductor.rs`,
which rewrites every declared gate's command to `true` and asserts only the decomposition) and
`regenerate_config_round_trips_through_the_real_on_disk_loader_with_back_compat`
(`tests/integrate_conflict_merge_periphery.rs`) load it through `config_store::load`, so the
requirement check runs on it; the stage and agent assertions of
`project_events_reads_this_projects_own_real_workflow_yml` read it through
`config_store::load_workflow`, and `handbook_config_example_reproduces_the_repo_grounder_default`
(`tests/handbook_grounder_accuracy.rs`) reads its `grounder:` line as text, neither validating.
The Goal's claim is therefore scoped to text: adding or removing a gate in `.rigger/workflow.yml`
changes no Rust source and no test text, and the three retained tests that load this repository's
workflow through the validating `config_store::load` need, at run time, whatever executables that
workflow's `requires` names, which is the environment, not the test. None of the three needs one,
because this repository's workflow declares no `requires` (Deferrals). Pins on this repository's
`.rigger/agents/` prose (the persona pins criterion 8 moves and
`every_persona_carries_its_principle_gate_checklist_line`) are outside that claim, because no
persona pin reads `.rigger/workflow.yml`; a persona edit is the operator's own text change and its
pin moves with it.

**THE GATE SCRATCH ROOT IS HANDED GENERICALLY, decided here.** Every gate that runs for a unit is
handed `RIGGER_GATE_SCRATCH` (`GATE_SCRATCH_ENV`, beside `STORE_FENCE_ENV` in
`crates/rigger-gates-shell/src/gate.rs`) naming `<scratch root>/rigger-gate-<slug>`: the
`unit_sibling` of the unit worktree under `UNIT_GATE_SCRATCH_PREFIX` (`"rigger-gate-"`, in
`crates/rigger-domain/src/spawn.rs`). `run_gates_at` and `run_regenerate_command` derive it where
they derive the mutants root today, from the `dir` their caller passes. `ExecRunner::run` sets the
variable when the derived path is non-empty and, when it is empty, removes it from the child's
environment (`Command::env_remove`), so no gate inherits a root from the process that runs it:
this repository's own `test` gate runs the suite with the variable set to the outer unit's root,
and an inner rigger's gate on a review worktree, or with no worktree, still runs without it. The
unset case is asserted with the variable present in the test's own process
(`exec_runner_removes_an_inherited_gate_scratch_root_when_handed_none`). Set: `run_single_stage`'s
three gate passes and `run_speculation`'s two (through `run_gates`) pass the unit's or the lane's
`rigger-wt-` worktree, and `regenerate_conflicted_paths` passes the worktree it integrates, so each
gate gets that worktree's sibling; `integrate_and_emit`'s post-merge re-gate passes its throwaway
`Throwaway::POSTMERGE` worktree under `GateSelection::PostMerge`, whose fallback to the unit's own
worktree path (`unit_worktree_dir`) sets the variable to that unit's sibling, as it sets
`CARGO_TARGET_DIR`. Unset: `run_fan_out_review_loop` passes a standalone review stage's
`rigger-review-` worktree, a caller with no worktree passes the empty `dir`, and
`run_deferred_gates` passes the empty `gate_scratch` beside its empty `dir`, so a deferred gate
runs with the variable removed. `run_deferred_gates` is the third `Runner::run` call site, beside
`run_gate_with_taxonomy` (the first run and each rerun of a gate `run_gates_at` runs) and
`run_regenerate_command`, and no other code outside tests calls `Runner::run`. The `Runner::run`
parameter `mutants_dir` becomes `gate_scratch` in every implementation (`ExecRunner`, the
conductor's test runners, `ReplayRunner` in `src/cli/mod.rs`, the runners in
`tests/integrate_conflict_merge_periphery.rs`), and `RecordingRunner`'s `mutants_dirs` becomes
`gate_scratches`. The conductor tests criterion 3 edits assert, through those recorded
`gate_scratches`, that the runner was handed the empty gate scratch for a standalone review stage's
gate pass (`run_fan_out_review_loop`) and for a deferred gate (`run_deferred_gates`), in the two
tests OTHER TEST DISPOSITIONS names for it. rigger never creates the root; a gate that uses it
creates it. A per-gate root, or
any path or variable keyed by a gate id, is NOT this root. A gate creates and removes only the names
it, or a tool it runs, created under the root, so the gates sharing a unit's root never collide.
`.rigger/gates/mutation.sh`'s root line is its first command,
`MUTANTS=${RIGGER_GATE_SCRATCH:?is empty or unset - this gate runs only for a unit}`: an empty or
unset `RIGGER_GATE_SCRATCH` refuses there, with the shell's message naming `RIGGER_GATE_SCRATCH`,
before any read or write. The anchor is derived after that check, at
`${MUTANTS%/*}/mutation-anchor` beside the root, with no `/nonexistent` default, and the `rm -rf`
before its first pass names `"$MUTANTS"` where it expanded `"${MUTANTS:?}"`: the root line's check
replaces both. Under the root it creates `rerun/` (its by-name rerun's `--output`), `rerun.args`,
`rerun.re`, `rerun.all`, `rerun.list`, `rerun.todo`, `rerun.diff`, `examined.txt` and `last.new/`,
and it hands the root to cargo-mutants as `TMPDIR`, where cargo-mutants makes its `cargo-mutants-*`
build copies and the tests it runs make their temporary files. That `rm -rf` removes under the root
`rerun`, `rerun.args`, `rerun.re`, `rerun.all`, `rerun.list`, `rerun.todo`, `rerun.diff`,
`last.new`, `examined.txt` and the `cargo-mutants-*` copies, whose glob stays because cargo-mutants
picks each copy's name, and names nothing else there; it also removes the worktree's `mutants.out`
and `mutants.out.old`, and the first pass's new `mutants.out` stays in the worktree. The script's
other `rm -rf` removes the anchor beside the root before `last.new/` moves into its place. Its
gate-environment paragraph names `RIGGER_GATE_SCRATCH` and `unit_sibling`. A `mutation.sh` an
earlier binary scaffolded into a consumer project reads `$MUTANTS` from its environment, which no
gate is handed: wired, that earlier script fails at its `${MUTANTS:?}` expansion with the shell's
own message naming `MUTANTS`. That script is the consumer's to replace: delete
`.rigger/gates/mutation.sh` and rerun `rigger init`, which writes the matched set's absent files, as
the handbook's check-in mutation subsection says (THE HANDBOOK).

**THE GATE SCRATCH ROOT HAS ONE LIFECYCLE, decided here.** Naming the root and classifying it at
every scratch walk is one lifecycle: a root nothing reclaims is NOT an implementation. One
predicate, `unit_scratch_slug(name)` in `crates/rigger-domain/src/spawn.rs`, returns the text
after a `cargo-target-` or `rigger-gate-` prefix (empty included, as
`strip_prefix(UNIT_CACHE_PREFIX)` answers today), and every scratch walk classifies by it:
`reclaim_orphan_scratch`'s per-unit arm, `scan_residue`, `scratch_totals`, `scratch_footprint`'s
dead filter and `find_shadow_stores`'s prune. A `rigger-gate-<slug>` whose unit is not live is
therefore reported, measured and reclaimed in the per-unit caches category as its cache sibling
is, and a live unit's is spared. A walk that matches `cargo-target-` or `rigger-gate-` with its own
`starts_with` or `strip_prefix` is NOT an implementation. One function,
`reclaim_gate_scratch_sibling(worktree_dir, authorized_root)` in
`crates/rigger-worktree-git/src/worktree.rs`, reaps
`unit_sibling(worktree_dir, UNIT_GATE_SCRATCH_PREFIX)` through `reap_dir_before_removal` and then
removes it, the reap-then-remove pair `reclaim_cache_sibling` runs on each sibling it reclaims;
`reclaim_cache_sibling` calls it in place of its mutants-root arm, so the root goes when the unit's
worktree is removed, and `UNIT_MUTANTS_PREFIX` is deleted. The post-merge re-gate's root is the
sibling of `unit_worktree_dir(&scratch, &st.name)`. On the single-lane path `run_stage` removes the
worktree at that path only after `run_single_stage`, and with it `integrate_and_emit`'s re-gate,
has returned, but `run_speculation` removes a lane whose implementer spawn errored before any
re-gate runs, and lane 0's worktree sits at that path, so a re-gate can run after the removal that
reclaims its root. `integrate_and_emit` therefore calls `reclaim_gate_scratch_sibling`, which is
`pub`, on that path, with the scratch root as its authorized root, whenever it took its
`!commit.is_empty()` re-gate branch and that branch's outcome passed (`merged.pass`), whether
`run_gates_at` answered the verdicts from the record - an exact-key replay from `gate_verdicts`
under `GateKey::PostMergeVerdict`, or a content-addressed cache-hit from `green_digests` - or ran a
gate command: a resumed step whose re-gate replays still reclaims the root, so no root outlives its
unit's landing, the check-in unit included. `run_speculation`
evaluates its candidates one at a time in lane order and returns at the winner, and every
`Runner::run` call returns only once its gate command exits (`ExecRunner::run` waits on it through
`Command::output`), so every lane's gate passes have returned before the winner's
`integrate_and_emit` runs: the post-merge reclaim of `unit_worktree_dir`'s sibling, which is also
lane 0's root, runs while no gate pass of the unit is running, whether lane 0 was removed on an
implementer error or still stands when lane 1 wins, and a process a gate left with its working
directory under that root is ended by `reap_dir_before_removal` before the removal. The call runs
after `integrate_mu` is released: `integrate_and_emit`'s guard on it is held from the landing merge
through the re-gate, the reindex, the `FILE_TOUCHED` emits and `mark_stale_downstream`, and
`integrate_and_emit` drops that guard once `mark_stale_downstream` returns, then reclaims, then
returns its `Integration`, so a sibling landing waiting on the lock never waits on a reap. The
post-merge call reclaims that one sibling only: the unit's `cargo-target-<slug>` cache and its
store-fence sibling stay with the worktree's removal exactly as today, so a straggler lens still
working the unit after it integrated keeps its warm cache, and the worktree removal's own exposure
to such a straggler is unchanged and outside this spec. The one-sibling function serves both
callers rather than `reclaim_cache_sibling` gaining a parameter naming the siblings to reclaim,
because that selector is an argument every existing caller (`Worktree::remove`,
`Worktree::discard`, `sweep_terminal_logged`, `reclaim_worktree_on_branch`) would pass only to
name the full set, while one shared function keeps the derivation and the reap-then-remove pair in
one home. A red re-gate lands nothing and leaves the root to the unit's remaining lifecycle, its
worktree's removal or the backstop once the unit is not live.

**CRITERIA 3 AND 4 SPLIT AT THE UNIT SIBLING PREFIX.** Criterion 3 adds `UNIT_GATE_SCRATCH_PREFIX`,
moves `run_gates_at`, `run_regenerate_command` and their tests onto it, and renames
`unit_mutants_sibling_maps_a_unit_worktree_to_its_mutants_root_and_ignores_the_rest` to
`unit_gate_scratch_sibling_maps_a_unit_worktree_to_its_gate_scratch_root_and_ignores_the_rest` on
it. That test exercises `unit_sibling`, the one generic sibling helper, which both criteria keep and
which gains no gate-specific derivation wrapper: `run_gates_at`, `run_regenerate_command` and
criterion 4's `reclaim_gate_scratch_sibling` each call it with `UNIT_GATE_SCRATCH_PREFIX`; the
script's `worktree::unit_mutants_sibling` citation becomes `unit_sibling` in criterion 3's
gate-environment paragraph. Criterion 4 adds `reclaim_gate_scratch_sibling`, moves
`reclaim_cache_sibling` onto `UNIT_GATE_SCRATCH_PREFIX` through it, adds its post-merge call in
`integrate_and_emit` (THE GATE SCRATCH ROOT HAS ONE LIFECYCLE), deletes `UNIT_MUTANTS_PREFIX` and
renames the reaper tests that name it. The `integrate_and_emit` reclaim call, its test
`a_passing_post_merge_re_gate_reclaims_the_gate_scratch_root_its_fallback_names` and the
reclamation sentence of `.rigger/gates/mutation.sh` are part of criterion 4's one-lifecycle
headline: a criterion-4 unit diff lacking any of the three does not satisfy the criterion.
Criterion 4 also owns every reclamation sentence of
`.rigger/gates/mutation.sh`: INCREMENTAL RE-SWEEPS' sentence that the anchor lives outside
`$MUTANTS` because that root is reclaimed with the unit, whose reason it rewrites to
`that root is reclaimed with the unit's worktree and at a passing post-merge re-gate`, its example
kept. That sentence is then the one place the script says when the root is reclaimed: the
gate-environment paragraph's clause that the conductor reaps the root at unit terminus is gone
from criterion 3's rewrite, which makes no reclamation claim (CRITERIA 3 AND 5 SPLIT AT
`.rigger/gates/mutation.sh`), and INCREMENTAL RE-SWEEPS' `a reclaimed scratch root` names the
scratch root's own removal, which no criterion changes, and stays. Between the two landings
`UNIT_MUTANTS_PREFIX` keeps one production reader, `reclaim_cache_sibling`, so it is not dead code,
no gate this repository runs writes the new root, since no stage of its workflow lists `mutation`,
and the script's reclamation sentence stays as written until criterion 4 rewrites it. The root
assertion of `the_post_merge_re_gate_runs_in_its_own_scratch_worktree_never_the_repo`, which
criterion 3 re-points, reads the handed path only: its gate appends the variable's value to a log
outside the repository, and the test compares that logged string with each unit's
`rigger-gate-<slug>` sibling, never reading or testing a path under the root, so criterion 4's
post-merge reclaim cannot break it.

**THE TEMPLATE SETS, decided here.** A template set is a directory `scaffold/<key>/` at the
repository root holding two files. `set.yml` declares `detect` (marker file names at the project
root), `gates` (the text of the `gates:` block, comments kept), `implement` and `checkin` (the two
stages' gate lists); `src/cli/setup.rs` parses it with `serde_yaml`, every key required and unknown
keys refused. One function, `parse_template_set`, parses a set: its `set.yml`, then its `gates` text
as a YAML mapping whose keys are the declared gate ids. A `gates` text that is empty or holds only
comments is the empty mapping: `parse_template_set` returns that set with its `gates` empty, which
renders `gates: {}`, so both of that set's lists must be `[]`, since a list naming any id names one
the empty mapping does not declare. `rigger init` refuses, naming the set key and the error, and
writes nothing - no directory, no file, not the workflow - when a set's `set.yml` or `gates` text
fails to parse or its `implement` or `checkin` list names a gate id its `gates` text does not
declare: `init_project` parses every embedded set before its first write, since detection and the
no-set line read every set's `detect`. The Rust set's gate ids and stage lists have one pin, the
init test `a_scaffolded_rust_project_carries_the_rust_sets_gates_and_every_checklist_line` in
`tests/principle_gates_wiring.rs` (Notes); `src/cli/setup.rs`'s scaffold test,
`scaffold_parses_into_a_valid_config`, names no gate id and asserts that every embedded set parses
through `parse_template_set`, every rendering (each set's and the no-set one) loads and
validates, no rendered gate command is a placeholder, and every id a stage list names is declared,
so a bad shipped set fails this repository's tests, never a consumer's init. `files` lists one
repository-relative path per line, blank lines skipped; each path is written to the same path in
the scaffolded project, its parent directories created as needed. One function,
`generate_template_sets` in `build/template_sets.rs` (included by `build.rs` through `#[path]` as
`build/gitsemver.rs` is), enumerates the entries under `scaffold/` in name order, skipping every
entry that is not a directory, and returns in that one enumeration both the source of
`$OUT_DIR/template_sets.rs` - a `TEMPLATE_SETS` array holding one `TemplateSet` per set directory
with its key, `include_str!` of its `set.yml` and `include_str!` of each listed file - and its watch
paths: `scaffold/`, both files of every set and every listed file. `build.rs` writes that source and
prints `cargo:rerun-if-changed` for each returned watch path, beside the lines it prints for its own
module files, `build/template_sets.rs` among them, and enumerates `scaffold/` nowhere itself. A set
directory missing `set.yml` or `files` fails the build naming the set, a listed path that is
absolute, holds a `..` segment or names no file fails it naming the set and the path, and a
repository root with no `scaffold/` directory, or a `scaffold/` holding no set directory, fails it
naming `scaffold/`: `generate_template_sets` returns the refusal and `build.rs` fails with it.
rigger ships at least the Rust set, so an empty catalogue is a broken tree, never a valid binary
(the build provenance, by contrast, is optional, and `build.rs` falls back to `unknown` outside a
git checkout), and the no-set line's parenthetical therefore always lists at least one set.
`generate_template_sets` is pure over the repository root, and `tests/template_sets_build.rs`
includes `build/template_sets.rs` by `#[path]`, as `tests/build_watch_paths.rs` includes
`build/watch.rs`. `src/cli/setup.rs` `include!`s the generated file. The embedded bytes are data
`rigger init` copies; no non-test code in `src/` or `crates/` names a gate id or a gate script as a
path, a command, a `match` arm or an embedded file; prose that names a gate (the built-in
instruction files, persona seeds, comments) is governed by OUT OF SCOPE. The non-test code that
names one today is, in `src/cli/setup.rs`, `SCAFFOLD_WORKFLOW`, `SCAFFOLD_GATE_FILES`,
`init_project`'s `gates` directory join and `scaffold_summary_lines`'s `.rigger/gates/` line, all
criterion 5's, which deletes or rewrites each as this block decides, and `MUTATION_GATE_ID` with
its readers (`Config::validate`'s probe call, `cmd_validate`, `build_environment_report` and the
`src/cli/mod.rs` import), criterion 1's deletions; any other such site a reviewer finds belongs to
criterion 5, and rule 4's `mutation` tokens stay criterion 8's (THE CORE NAMES NO GATE). Why files embedded by the
build script: each shipped script keeps one home, its file under this repository's `.rigger/gates/`,
where an embedded copy in Rust text would be a second home; `src/` then embeds no gate script, where
an `include_str!` in `src/` would name `mutation.sh`; and the binary stays self-contained, where
files read at run time would need an installed location beside the binary, a moving part
`rigger init` does not have. `set.yml` stays text parsed at run time so the build script needs no
parsing crate. A copy of a script under `scaffold/` or in Rust text is NOT the template set; the
listed file is.

The key is the set directory's name. `init_project` picks the first set in key order any of whose
`detect` markers is a file at the project root; there is no language flag. `SCAFFOLD_WORKFLOW`
carries three placeholders - `@GATES@` on its own line where the `gates:` block goes,
`@IMPLEMENT_GATES@` and `@CHECKIN_GATES@` as the two stages' `gates:` values - and one pure
`scaffold_workflow(set)` renders it: with a set, `gates:` followed by the set's `gates` text
indented two spaces, or `gates: {}` when that `gates` is empty, and the two lists as YAML flow
sequences; with none, `gates: {}` and `[]` for both lists. Its head comment, its check-in comment
and its check-in coverage drop the mutation sweep for the language-neutral text in Notes. The
workflow is written only when absent, as today; the matched set's files are written when absent
whether or not the workflow was. `ScaffoldReport` replaces `new_gate_files` with `new_set_files`
(the paths written) and gains `gate_set` (the matched key). `scaffold_summary_lines` prints
`scaffolded <path>` per written file, names the set on the workflow line when one applied, and
prints the no-set line (Notes) when the workflow was written with none. `SCAFFOLD_GATE_FILES` is
deleted. A placeholder is a gate command that is empty, `true`, `:` or ends in `; true`, and a
project matching no set gets no gate rather than a stand-in. The no-set rendering validates, since
no `Config::validate` check requires a gate. Its fan-out units carry no gate and run under the
authored-ungated carve-out of `assert_no_ungated_fanout_unit`, which refuses an ungated unit only
when its template declares gates, and `rigger validate` names the template in its non-fatal
`ungated_fan_out_templates` warning. They and its check-in unit are still reviewed exactly as a
gated unit is: over an empty gate list `run_gates_at` returns green with no gate run, and
`review_unit` then runs the panel `Workflow::effective_review_panel` gives, the rendering's
`defaults.review`. The Rust set (Notes) wires `fmt`, `build`, `test`, `lint` and `red-before-green`,
`fmt` first on both stages as this repository's own stages list it, ships
`.rigger/gates/mutation.sh` unwired with the `.rigger/gates/container-env.sh` it sources, and shows
in a comment the mutation gate's declaration with its `requires` and the `checkin` list that wires
it after `test`. That `requires` lists every executable the script runs beyond the shell utilities
and the `cargo` and `git` the set's own gates already run - `cargo-mutants`, and `cargo-nextest`,
which cargo-mutants runs under `--test-tool nextest` (`systemd-run` and the container CLI are probed
and fall back) - so a consumer who wires it as shown fails at validate, never mid-run. It scaffolds
no `boundary` or `audit` gate: neither has a per-language implementation, and a stand-in is banned.
The scaffolded adjudicator seed's checklist line becomes `A red gate is non-negotiable: never
weaken, skip or re-wire a gate to get green.`, naming no gate the scaffold may lack, and that
sentence replaces the `boundary` line as `PERSONA_CHECKLIST`'s adjudicator entry. This repository's
adjudicator persona carries the same sentence beside its own `boundary` sentence, which names a gate
this repository wires, so `every_persona_carries_its_principle_gate_checklist_line` stays whole:
through `missing_checklist_lines`, which matches each entry as a substring of its persona file, the
repository check asserts every `PERSONA_CHECKLIST` entry on the persona it names under this
repository's `.rigger/agents/`, and the scaffold check asserts the same table on the seeds `rigger
init` writes. Every `src/cli/setup.rs` test that reads `SCAFFOLD_WORKFLOW` reads `scaffold_workflow`
output instead, every rendering where it asserts the workflow loads. A test that needs gates in a
scaffolded project declares them in its own fixture workflow.

**THE RUST SET'S RED-BEFORE-GREEN IS THIS REPOSITORY'S SCRIPT, decided here.** The Rust set's
`files` lists `.rigger/gates/red-before-green.sh` and its `red-before-green` gate runs
`sh .rigger/gates/red-before-green.sh`, so the shipped template is the file this repository's own
`red-before-green` gate runs: one source of truth, nothing generated, no drift check. Its default
run branch, `rigger-run`, is the run branch rigger anchors in every project (`RUN_BRANCH`). Its
source rule holds in any Cargo layout: `is_source_path` matches any file with a `src/` path
component (`src/...`, `crates/<name>/src/...`, `tools/<name>/src/...`), so the gate can fail
wherever a workspace keeps its members, and every file it matches today stays source;
`is_test_path` and the test-hunk rule are unchanged, and the script's header names the new rule.
The build scripts (`build.rs`, `build/gitsemver.rs`, `build/watch.rs`, `crates/rigger-dash/build.rs`,
`crates/rigger-dash/build/console_wasm.rs` and criterion 5's `build/template_sets.rs`) have no
`src/` component and stay outside the rule. By design, any file with a `src/` path component is
source, Rust or not, exactly as every file under this repository's `src/` and `crates/<name>/src/`
is today (`crates/rigger-dash/src/dash.html`, the fonts under `crates/rigger-console/src/console/`
and the instruction `.md` files under `crates/rigger-domain/src/instructions/` are gated); a
consumer whose documentation lives under a `src/` component (an mdBook's `book/src/`) edits its own
copy of the script, which is theirs after `rigger init`. Criterion 6's scaffolded-command case (a
test-less `src/` commit fails, a branch whose test commit comes first passes) is a behaviour pin:
`is_source_path` matches `src/*` today, so the case is green when unit 6 starts and pins what the
shipped script already does, never code written before its test. The unit's red is the layout case
(`tools/x/src/lib.rs`): today's script passes that test-less commit, so the case fails until the
source rule widens.

**THE SPAWN-KEYED CACHE-HOME ROOT IS DELETED, decided here.** Deleted whole:
`MUTATION_SCRATCH_SUBDIR`, `mutation_scratch_path`, `reclaim_unit_mutation_scratch` and
`mutation_scratch_root` in `crates/rigger-driver/src/driver/replay.rs`;
`reclaim_terminal_unit_mutation_scratch` and `mutation_scratch_settled` with their call sites in the
conductor (`run_stage`'s teardown, `run_speculation`'s three exits and
`gc_integrated_branches_logged`'s per-unit loop); the cache-home half of
`reclaim_spawn_registered_scratch` in `src/cli/run.rs`; and the `mutation_root` parameter of
`footprint_report` with its computation in `measure_footprint`. `rigger result` then reclaims only
the reporting spawn's agent scratch, and the registered scratch roots category measures
`agent-scratch` alone. `cache_home_from` stays: `worktree.rs` and `hygiene.rs` read it.
`mutation_scratch_settled` reads only run state folded from the log (`ledger::RunState::is_terminal`
and the unit's `ledger::Status`) and the workflow's stage `on_pass`, never a file or a process,
and neither it nor the reclaim it gates emits anything, so deleting them and their call sites
changes no emitted event, no fold and no replay key, and a step resumed after a crash that ran or
skipped the settled reclaim before the deletion behaves as every other resumed step, since the
deleted path only ever removed a directory nothing writes. A test whose subject is the root is
deleted, with every helper only deleted tests call; a test that also asserts agent-scratch,
footprint, reset or reap behaviour keeps those assertions (Notes). The unit's red is the kept tests
that call `footprint_report` (Notes), trimmed to call it without its `mutation_root` argument and
committed before the deletion: today's `footprint_report` takes that argument, so the test target
holding them fails to compile until the deletion removes it, which makes them green; no other kept
test's call changes. `tests/simplification_audit.rs`'s report drops the boundary violation whose
subject is `reclaim_terminal_unit_mutation_scratch`, the plan item that relocates it and every
sentence and assertion that counts or cites them, because its `cite_fn` panics on a deleted
function, and the regenerated `docs/audit/` files land with it.

**THE SIMPLIFICATION AUDIT'S CITATIONS, decided here.** A criterion that deletes or renames an item
the simplification audit report cites owns that citation's rewording and the regenerated
`docs/audit/` files in the same unit. The report's prose cites functions through `cite_fn` in
`tests/simplification_audit.rs`: `process_state`, `pid_starttime`,
`reclaim_terminal_unit_mutation_scratch`, `walk_batches`, the three `project_batches`,
`content_hash`, `reap_then_remove_worktree` and `materialize_config_at_rev`. Criterion 7 deletes one
of them, `reclaim_terminal_unit_mutation_scratch` (THE SPAWN-KEYED CACHE-HOME ROOT IS DELETED);
criteria 1, 2, 3, 4, 5, 6 and 8 delete or rename none, and the report's `line_of` needles name
nothing any criterion changes. The generated sections cite by scan, so a unit that deletes, renames
or moves a scanned function only regenerates them.

**THE CORE NAMES NO GATE, decided here.** `tests/boundary_audit.rs` gains rule 4: no file under
`src/` or `crates/`, of any extension and read as lossy UTF-8, holds a line containing,
case-sensitively, `cargo-mutants`, `cargo mutants`, `mutation.sh`, `rigger-mutants`, `MUTANTS` as a
whole word, `MUTATION_GATE_ID`, the string literal `"mutation"` or the concept id `gate:mutation`,
and no `.rs` file under them holds the YAML key form: a line that begins inside a string or raw
string literal an earlier line opened and whose first non-blank characters are `mutation:`. Rule 4
enumerates those files through the walk rules 2 and 3 enumerate theirs through and adds no second
enumeration. That walk, the recursive directory walk of `collect_files_with_extension` in
`tests/common/repo.rs` (which `collect_rs_files` wraps for `.rs`), skips nothing it can read, so
rule 4 reads untracked and ignored files as rules 2 and 3 do; it moves into `collect_files`, which
lists every file, `collect_files_with_extension` filters that list by extension, and rule 4 takes
`collect_files` of `src/` and `crates/` unfiltered. A file of any extension under `src/` or
`crates/` that holds a rule-4 token, an untracked or ignored leftover included (a merge `.orig`, an
editor backup), is a red rule 4 by design: the failure line names its path, and there is no skip
list. A `MUTANTS` match needs a character outside
`[A-Za-z0-9_]`, or the line edge, on both sides:
`$MUTANTS` and `"MUTANTS"` are hits, `UNIT_MUTANTS_PREFIX` and `MUTANTS_DIR` are not. Rule 4 finds
those lines with the string, char-literal and comment lexing `tests/simplification_audit.rs` scans
by (`skip_string_literal`, `char_literal_len`, `skip_block_comment`), moved into
`tests/common/source_audit.rs` so one lexer serves both suites; a second lexer is NOT an
implementation, and a file of another extension holds no string literal. Violations are reported by
file and line, and the rule carries no allowlist; its fixture is rule 4's row in OTHER TEST
DISPOSITIONS. A token or form split across `concat!`, `format!` or adjacent literals, or built at
run time, to pass the rule is NOT a removal. Rule 4 is the mechanical check for every spelling of
the gate id `mutation` this spec bans under `src/` and `crates/`: a quoted or raw string literal
(`r"mutation"` and `r#"mutation"#` hold `"mutation"`), a YAML key and a concept id; the two places a
workflow may declare a gate under that id are this repository's own `.rigger/workflow.yml` and the
fixtures under `tests/`. The retired key `build.mutation` is a config key, not a gate id, keeps its
refusal and matches none of rule 4's forms: Rust names it as the dotted key or the
`BuildConfig::mutation` field, whose `mutation:` lines in `build_environment_report`'s tests are
code, not string text, and the one string that sets it, in
`build_config_parses_mutation_and_defaults_to_empty_when_omitted`, holds `build:\n  mutation: on\n`
on one line. The Goal's count of 130 lines was taken with the seven original tokens; the twelve
lines rule 4's two added forms match are additional: `gate:mutation` in
`crates/rigger-grounder/src/grounder/workflowdef.rs` (its module doc,
`stages_gates_and_agents_all_become_concepts`, `needs_and_runs_edges_match_the_yaml_lists` and
`project_events_reads_this_projects_own_real_workflow_yml`), on six lines of
`workflow_definition_events_fold_into_stage_gate_agent_nodes_with_needs_runs_reviews_edges` in
`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` and in the `REL_RUNS` doc in
`crates/rigger-domain/src/contextgraph.rs`, and the `mutation` gate's YAML key in
`SCAFFOLD_WORKFLOW` (`src/cli/setup.rs`). Criterion 2 deletes the live-workflow test's
`gate:mutation` assertion, criterion 5's rendering drops the gate's line, and criterion 8 removes
the rest (below). The id's other spellings in code under `src/` and `crates/`, which rule 4 does not
match, go with items criteria 1 and 5 delete or rewrite: `gates.mutation` in
`MutationBinaryUnavailable`'s message, the escaped `\"mutation\"` in `build_environment_report`'s
tests, the backquoted `mutation` gate in the `build.mutation` refusal's reason and the `checkin`
list in `SCAFFOLD_WORKFLOW`. Each criterion rewrites the comments on and in the items whose code it
changes; criterion 8 rewrites every other comment under `src/` and `crates/` that names a banned
token or a symbol this spec deletes to name the generic mechanism instead, and a comment that names
a gate with neither is prose and stays (OUT OF SCOPE).
Criterion 8 removes every occurrence criteria 1, 3, 4, 5 and 7 leave:

- `REVIEWER_DISCIPLINE`'s last sentence becomes
  `Never run a mutation sweep, directly or through a verify helper: mutation testing belongs to the gate that sweeps, never to a review.`,
  and `every_reviewer_prompt_forbids_cargo_mutants` becomes
  `every_reviewer_prompt_forbids_a_mutation_sweep`.
- `implementer_persona_pins_the_checkin_stage_survivor_closing_contract`,
  `implementer_persona_pins_the_checkpoint_before_long_work_contract` and
  `no_persona_under_rigger_agents_invokes_cargo_mutants`, with `implementer_persona_normalized` and
  `assert_implementer_persona_pins`, move from `src/cli/mod.rs` into
  `tests/principle_gates_wiring.rs`, their assertions unchanged.
- The fixture gate id in the `crates/rigger-grounder/src/grounder/workflowdef.rs` and
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` tests becomes `sweep`, its command
  `sh sweep.sh`, and its concept id `gate:sweep` wherever those tests spell `gate:mutation`; the
  comments that spell `gate:mutation` (the `workflowdef.rs` module doc, the `sqlite.rs` test's
  comment, the `REL_RUNS` doc) name `gate:<name>` instead and drop their claim that the example is
  the Design text's own; the fixture root of
  `is_reapable_base_accepts_a_root_that_is_not_named_dot_rigger_tmp_at_all` is renamed from
  `rigger-mutants`.
- The hits the Goal counts in `crates/rigger-grounder/src/grounder/symbols/extract.rs` (the doc
  comments of `predicate_group_facts` and of the test case
  `a_not_wrapping_a_non_test_atom_never_marks_the_item_test`), `crates/rigger-dash/src/dash.rs` (the
  doc comment of the test case `dash_serving_pid_on_is_none_when_the_pid_header_value_is_not_a_number`)
  and `src/cli/dashboard.rs` (a comment in `cmd_dash`) are comments, none a fixture string or a
  classifier; each says `mutation testing`, `the mutation tool` or `a mutation sweep` where it
  names the tool.
- `docs/architecture-addendum-world-authority.md` names `unit_sibling` where it named
  `mutation_scratch_path`.

**THE HANDBOOK, decided here.** `docs/handbook/authoring-loops.md` is hand-written - no `rigger`
render produces it - and three criteria edit disjoint parts of it with the text in Notes: criterion
1 adds the `requires` paragraph, which names `config_store::load` and the
`load_with_gate_requirements` it wraps as the validating loads that refuse a missing requirement
and `load_workflow` and `read_scratch_defaults` as reads that never validate, to the "The workflow"
section after its example; criterion 3 adds the gate scratch sentence to the "The per-unit
lifecycle" subsection; criterion 5 rewrites the "The check-in mutation sweep" subsection to say the
Rust set ships the script unwired, how to wire it and how a consumer replaces a script an earlier
binary scaffolded, and rewrites step 2 of the new-project checklist.

**CRITERIA 1 AND 2 SPLIT AT `.github/workflows/rust.yml`.** Criterion 1 owns the `cargo-mutants`
install and that step's comment; criterion 2 owns the `build-test` job's lanes comment
(LIVE-WORKFLOW TESTS).

**CRITERIA 1 AND 5 SPLIT AT `SCAFFOLD_WORKFLOW`.** Criterion 1 deletes only the sentence claiming a
gate under the id `mutation` requires `cargo-mutants`; criterion 5 owns the rest of the scaffold
text.

**CRITERIA 2 AND 5 SPLIT AT
`the_mutation_gate_runs_the_shipped_script_and_init_writes_the_same_script`.** Criterion 2 deletes
its first assertion, which reads this repository's `mutation` gate command, and the repository mode
of `missing_principle_gates`; criterion 5 replaces the rest of that test with
`init_writes_each_file_the_rust_set_lists` and deletes the rest of the principle-gate check, whose
scaffold half the Rust set's exact stage lists subsume (Notes).

**CRITERIA 3 AND 5 SPLIT AT `.rigger/gates/mutation.sh`.** Criterion 3 owns its root line, the two
expansions the root line's check replaces (the anchor line's `/nonexistent` default and the `rm -rf`
line's `${MUTANTS:?}`) and its gate-environment paragraph, whose root sentence describes only the
handing - `RIGGER_GATE_SCRATCH`, the `unit_sibling` of the unit worktree - and makes no reclamation
claim, and whose `$RIGGER_RUN_BASE` sentences describe the conductor's run base and stay as
written. Criterion 5 owns every sentence of the script that describes the scaffold, what a stage
carries or a stage order:

- the first paragraph, which names the gate, says the Rust set ships the script unwired and how to
  wire it, and names no project's workflow;
- THE VERDICT's last sentence, whose subject
  `The checkin stage's task text carries the survivor-closing protocol:` becomes
  `The survivor-closing protocol for a checkin stage that lists this gate:`, the protocol after the
  colon unchanged, since no stage carries a task text (`config::Stage` has none);
- THE BASELINE STAYS ON's first sentence, whose parenthetical `(the scaffold's does)` becomes
  `(as the Rust gate template set's comment shows it wired)`, true whether or not a consumer wires
  the gate, since the comment shows `test` before `mutation` in the `checkin` list (Notes).

Neither asserts the other's text.

**LANDING-ORDER SIMULATION** (ordered pairs sharing a surface; 1, 2, 3 and 7 in any order, 4 after
3, 5 after 1, 2 and 3, 6 after 5, 8 after 1, 3, 4, 5 and 7, 9 last. The implement stage's `audit`
gate regenerates `docs/audit/` on each unit's own tree, so every pair whose units change a function
the audit scans shares that directory and the first one's regenerated files hold on its tree; the
rows name it only beside the audit's own source. A pair absent here shares no other surface):

| First | Without | Shared surface | Does the first one's own text hold? |
|---|---|---|---|
| 1 | 2 | `Config::validate` over this repository's workflow, which a live-workflow test calls; `.github/workflows/rust.yml`; `tests/cli.rs`; `crates/rigger-conductor/src/conductor.rs`; `crates/rigger-grounder/src/grounder/workflowdef.rs` | yes - that workflow declares no `requires`, so its validation holds with no `cargo-mutants` on `PATH`; in `rust.yml` 1 edits the install step and its comment, 2 the `build-test` job's lanes comment, disjoint lines; in `tests/cli.rs` 1 rewrites the requirement tests, 2 deletes and re-homes the live-workflow tests; in `conductor.rs` 1 adds `requires` to the deferred-gate tests' `config::Gate` literals, 2 adds the fixture check-in test; in `workflowdef.rs` 1 adds `requires` to the fixture literals, 2 deletes the live test's gate assertion; disjoint items |
| 2 | 1 | the same | yes - 2 deletes that test, and its conductor fixture declares no `requires`; the same disjoint `rust.yml` lines and items |
| 1 | 3 | `crates/rigger-gates-shell/src/gate.rs`; the handbook; `crates/rigger-conductor/src/conductor.rs`; `src/cli/mod.rs` | yes - 1 changes the probe items, `path_has_executable` and their tests, 3 changes `ExecRunner::run` and the `Runner` doc and adds the removal test; in `conductor.rs` 1 adds `requires` to the deferred-gate tests' `config::Gate` literals, 3 edits `run_gates_at`, `run_regenerate_command`, the runner parameter and the gate-environment tests; in `src/cli/mod.rs` 1 drops `MUTATION_GATE_ID` from the `rigger::gate` import, 3 renames `ReplayRunner`'s parameter; 3's `gate_scratches` assertion in `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` sits in that test's body, never the `config::Gate` literal of `deferred_gate_run` 1 edits; disjoint items and sections |
| 3 | 1 | the same | yes - the same disjoint items and sections |
| 1 | 4 | `crates/rigger-conductor/src/conductor.rs`; `src/cli/mod.rs` | yes - in `conductor.rs` 1 adds `requires` to the deferred-gate tests' `config::Gate` literals, 4 adds the post-merge reclaim call in `integrate_and_emit` and its test; in `src/cli/mod.rs` 1 drops `MUTATION_GATE_ID` from the `rigger::gate` import, 4 changes the scratch walks and their tests; disjoint items |
| 4 | 1 | the same | yes - the same disjoint items |
| 1 | 5 | `SCAFFOLD_WORKFLOW`; `rigger validate` on a fresh scaffold; the handbook; `tests/cli.rs` | yes - the placeholder `mutation` gate declares no `requires`, so a fresh scaffold validates, and 1 deletes only its stale sentence; in `tests/cli.rs` 1 rewrites the requirement tests, 5 edits only the test that needs gates in a scaffolded project, disjoint items |
| 5 | 1 | the scaffold comment naming `requires` | not reachable - 5 needs 1 |
| 1 | 7 | `tests/cli.rs`; `src/cli/validate.rs`; `crates/rigger-conductor/src/conductor.rs`; `src/cli/mod.rs` | yes - 1 rewrites the requirement tests, `cmd_validate`'s load and gate lines and `build_environment_report`, 7 deletes the cache-home tests and `measure_footprint`'s root; in `conductor.rs` 1 adds `requires` to the deferred-gate tests' `config::Gate` literals, 7 deletes the terminal-unit reclaim and its call sites; in `src/cli/mod.rs` 1 drops `MUTATION_GATE_ID` from the `rigger::gate` import, 7 removes `footprint_report`'s cache-home input and trims its tests; disjoint items |
| 7 | 1 | the same | yes - the same disjoint items |
| 1 | 8 | the fixture `Gate` literals in `workflowdef.rs` | yes - 1 adds `requires` to each literal, 8 renames one literal's id and command |
| 2 | 3 | the live test asserting the script reads `$MUTANTS`; `RecordingRunner` in `crates/rigger-conductor/src/conductor.rs` | yes - 2 deletes the test and edits no script, and its fixture test reads the recorded gate ids, never the field 3 renames |
| 3 | 2 | the same | yes - the script keeps `$MUTANTS` as its own variable, set from `RIGGER_GATE_SCRATCH`, so the live test still finds it until 2 deletes it |
| 2 | 4 | `crates/rigger-conductor/src/conductor.rs` | yes - 2 adds the fixture check-in test, 4 adds the post-merge reclaim call in `integrate_and_emit` and its test; disjoint items |
| 4 | 2 | the same | yes - the same disjoint items |
| 2 | 5 | `missing_principle_gates`, `PRINCIPLE_GATES` and the mutation-script test in `tests/principle_gates_wiring.rs`; `tests/cli.rs` | yes - the scaffold mode still finds the placeholder `boundary`, `audit` and `red-before-green` gates on their stages; in `tests/cli.rs` 2 deletes and re-homes the live-workflow tests, 5 edits only the test that needs gates in a scaffolded project, disjoint items |
| 5 | 2 | the same | not reachable - 5 needs 2 |
| 2 | 6 | `tests/principle_gates_wiring.rs` | yes - 2 deletes repository-mode tests, 6 edits the red-before-green cases; disjoint items |
| 6 | 2 | the same | not reachable - 6 needs 5, which needs 2 |
| 2 | 7 | `tests/cli.rs`; `crates/rigger-conductor/src/conductor.rs` | yes - 2 deletes live-workflow tests and adds a fixture test, 7 deletes the cache-home tests and the terminal-unit reclaim; disjoint items |
| 7 | 2 | the same | yes - the same disjoint items |
| 2 | 8 | `tests/principle_gates_wiring.rs`; `workflowdef.rs`; `crates/rigger-conductor/src/conductor.rs` | yes - 2 deletes the live gate assertion and repository-mode tests, 8 renames the fixture and adds moved persona tests; in `conductor.rs` 2 adds the fixture check-in test, 8 rewrites `REVIEWER_DISCIPLINE`'s last sentence and renames its test; disjoint items |
| 8 | 2 | the same | not reachable - 8 needs 5, which needs 2 |
| 3 | 4 | `crates/rigger-domain/src/spawn.rs`; `crates/rigger-worktree-git/src/worktree.rs`; `crates/rigger-conductor/src/conductor.rs`; `src/cli/mod.rs`; `.rigger/gates/mutation.sh` | yes - 3 adds its prefix beside `UNIT_MUTANTS_PREFIX`, which `reclaim_cache_sibling` and its tests still read, and no gate this repository runs writes the root; in `conductor.rs` 3 edits `run_gates_at`, the runner parameter and the gate-environment tests, 4 adds the post-merge reclaim call in `integrate_and_emit` and its test; in `src/cli/mod.rs` 3 renames `ReplayRunner`'s parameter, 4 changes the scratch walks and their tests; in the script 3's gate-environment paragraph makes no reclamation claim and the reclamation sentence 4 rewrites stays as written; disjoint items |
| 4 | 3 | the same | not reachable - 4 needs 3 |
| 3 | 5 | `.rigger/gates/mutation.sh`; the handbook | yes - 3 edits the script's root line, the two expansions its check replaces and the gate-environment paragraph; init still writes the file from the same path |
| 5 | 3 | the same | not reachable - 5 needs 3 |
| 3 | 7 | `crates/rigger-conductor/src/conductor.rs`; `src/cli/mod.rs` | yes - 3 renames the runner parameter and edits `run_gates_at`, 7 deletes the terminal-unit reclaim and `footprint_report`'s input; disjoint items |
| 7 | 3 | the same | yes - the same disjoint items |
| 4 | 5 | `.rigger/gates/mutation.sh` | yes - 4 rewrites INCREMENTAL RE-SWEEPS' reclamation sentence, 5 the first paragraph, THE VERDICT's last sentence and THE BASELINE STAYS ON's first sentence; disjoint sentences |
| 5 | 4 | the same | yes - the same disjoint sentences |
| 4 | 7 | `src/cli/mod.rs`; `crates/rigger-conductor/src/conductor.rs` | yes - 4 changes which names `scratch_footprint` counts, 7 removes `footprint_report`'s cache-home input, and neither's tests seed the other's names; in `conductor.rs` 4 adds the post-merge reclaim call in `integrate_and_emit` and its test, 7 deletes the terminal-unit reclaim and its call sites in `run_stage`, `run_speculation` and `gc_integrated_branches_logged`, disjoint items |
| 7 | 4 | the same | yes - the same disjoint items |
| 5 | 6 | the scaffolded `red-before-green` gate; `.rigger/gates/red-before-green.sh`; `tests/principle_gates_wiring.rs` | yes - 5 asserts the gate is declared, listed and its script written with this repository's bytes, nothing about its verdict; 6 edits the script's source rule and the red-before-green cases, 5 lists the script in the Rust set and rewrites the scaffolded-project test; disjoint items |
| 6 | 5 | the same | not reachable - 6 needs 5 |
| 5 | 7 | `tests/cli.rs` | yes - in it 5 edits only a test that needs gates in a scaffolded project, moving it onto its own fixture workflow; the tests 7 deletes or trims there (the cache-home tests, the hostile spawn-id tests, the validate footprint tests) read no scaffolded gate: they write their own workflows or assert `rigger validate`'s footprint lines, which the no-set scaffold's ungated fan-out advisory does not match |
| 7 | 5 | the same | yes - 7 deletes and trims only those tests, and no test 5 adds or edits reads one of them; disjoint items |
| 5 | 8 | `tests/principle_gates_wiring.rs` | yes - 5 rewrites the scaffolded-project test, edits `PERSONA_CHECKLIST`'s adjudicator entry, which the kept repository check finds on this repository's adjudicator persona, and deletes the principle-gate helpers, 8 adds moved persona tests; disjoint items |
| 6 | 4, 7, 8 | this repository's own `red-before-green` gate, run on each later unit's branch | yes - every file a later unit touches under `src/` or `crates/<name>/src/` is source under both rules, and every tracked file outside `tests/` with a `src/` component is already under one of those, so no later unit's verdict changes |
| 6 | 3 | the same | not reachable - 6 needs 5, which needs 3 |
| 3, 4, 7 | 6 | the same | yes - none of them changes a file 6 asserts over |
| 6 | 8 | `tests/principle_gates_wiring.rs` | yes - 6 edits the red-before-green cases, 8 adds moved persona tests; disjoint items |
| 8 | 6 | the same | yes - 8 neither reads nor writes the red-before-green tests and changes no file 6 asserts over |
| 1, 3, 4, 5, 7 | 8 | rule 4 of `tests/boundary_audit.rs`; `tests/simplification_audit.rs` and `docs/audit/`; `crates/rigger-conductor/src/conductor.rs` (1's `config::Gate` literals, 3's runner rename, 4's post-merge reclaim and 7's deletions versus `REVIEWER_DISCIPLINE` and its test); `src/cli/mod.rs` (1's import, 3's `ReplayRunner`, 4's scratch walks and 7's `footprint_report` versus the persona pins moved out); each file where 8 rewrites a comment one of them leaves (THE CORE NAMES NO GATE) | yes - none of them asserts the core-wide scan, and 7's edits to `tests/simplification_audit.rs` touch its report, never the lexer 8 moves, each unit regenerating `docs/audit/` on its own tree; in every shared source file 8 edits only `REVIEWER_DISCIPLINE`, its test, the persona pins, its fixtures and the comments the others leave; disjoint items |
| 8 | 1, 3, 4, 5, 7 | rule 4 and every surface 8 shares with them | not reachable - 8 needs all five |
| 9 | any | the lanes over the integrated result | not reachable before the other eight - 9 needs all eight |

**CONSTRAINTS WALK.**
- *Criterion 1.* Empty: an absent or empty `requires` resolves nothing, validates, and reports
  `requires nothing`; a workflow with no gates prints no gate line; an unset `PATH` resolves every
  requirement missing; an empty or relative `PATH` component is skipped, so a requirement reachable
  only through one resolves missing and every `at <path>` is absolute. Repeated: a name listed
  twice resolves and prints twice; every `rigger step` re-runs the check through
  `config_store::load`; a workflow that also fails another check reports that check's message,
  since the requirement check runs last (Design). Reverted: a requirement installed again passes
  the next load. DROPPED: an
  executable removed from `PATH` mid-run makes the next step's load refuse with the message, before
  any gate runs; one removed after a load resolved it is reported where that load found it, since
  `rigger validate` renders the load's own list and makes no second lookup, so no report line can
  name an unresolved requirement; a `requires` entry deleted from a gate stops being checked at the
  next load; a symlink whose target is removed resolves missing. Concurrent: a read of `PATH` and
  file metadata only. Crash-resume: nothing is written, so a resumed step re-runs the check. Cold
  start: pure over the workflow and `PATH`; a `Config` handed to `conductor::run` without a
  validating load is never checked, so a gate whose requirement is missing fails at its own
  command (Design).
  Existing data: a workflow with no `requires` key parses
  unchanged; a workflow that declares a gate named `mutation` with no `requires` validates with no
  `cargo-mutants` anywhere; a symlinked tool directory on `PATH` resolves through its links; a
  `cargo` subcommand in `$CARGO_HOME/bin` with that directory off `PATH` resolves missing, by
  decision (Design); a named `build.wrapper` reachable only through an empty or relative `PATH`
  component, which loads today, is refused at load by `WrapperUnavailable`, and `build.wrapper:
  auto` stops injecting a wrapper reachable only that way (Design); the `build.mutation` refusal
  still names its key and spec 91.
- *Criterion 2.* Empty: out of scope - the fixture always has criteria. Repeated: two runs of one
  fixture record one order. Reverted, DROPPED: a gate added to or removed from this repository's
  workflow touches no test; that is the property the deletions buy. Concurrent: implement units may
  gate concurrently; the assertions compare stage-exclusive gate ids by recorded order, which holds
  under any interleaving of implement units. Crash-resume: out of scope - an in-process fixture run.
  Cold start: an in-memory store. Existing data: the deleted tests carry no data.
- *Criterion 3.* Empty: a gate on a standalone review worktree, or on a run with no worktree, runs
  without the variable, removed even when the gate's parent process holds it, and the post-merge
  re-gate gets its unit's root (Design); the shipped `mutation.sh` wired there refuses at its root
  line, naming `RIGGER_GATE_SCRATCH`, before any read or write. Repeated: every gate run of a unit
  is handed the same root, and each gate there touches only the names it created. Reverted: a unit
  resumed after escalation keeps its slug and so its root. DROPPED: a gate removed from a stage
  stops receiving the root, and what it left there goes with the root (criterion 4). Concurrent: two
  units, and two speculation lanes of one unit, get distinct roots because their worktree names
  differ; two gates of one unit share the root and never collide, each touching only its own names;
  each gate's variable follows its own `gate_scratch` alone, so a test that sets the variable in its
  own process changes no concurrent test's gate. Crash-resume: a step that dies mid-gate leaves the
  root in place, and the resumed unit's next gate is handed the same path. Cold start: derived from
  the worktree path alone, nothing stored. Existing data: a `cargo-mutants-<slug>` directory left by
  an earlier binary is never handed to a gate again (Notes); a `mutation.sh` an earlier binary
  scaffolded fails once wired, at its unset `MUTANTS`, and the consumer replaces it by deleting it
  and rerunning `rigger init` (Design); a nested run - this repository's own `test` gate runs the
  suite with `RIGGER_GATE_SCRATCH` set to the outer unit's root - hands an inner review-worktree or
  no-worktree gate no variable, because `ExecRunner::run` removes it when the derived path is empty.
- *Criterion 4.* Empty: a scratch root with no `rigger-gate-` entry is walked as today; a bare
  `rigger-gate-` is classified as a bare `cargo-target-` is today; an integration with no merge
  commit runs no post-merge re-gate and no post-merge reclaim. Repeated: a second walk over a
  reclaimed root reclaims nothing, and a second `reclaim_cache_sibling` on a reclaimed path removes
  nothing, as does a second `reclaim_gate_scratch_sibling`; a resumed step whose post-merge re-gate
  replays its recorded passing verdicts runs no gate command and still reclaims the root, since the
  reclaim follows the `!commit.is_empty()` branch and its passing outcome, never a gate run
  (Design). Reverted: a unit live again after a
  resume has its root spared by the next walk. DROPPED: a unit that reaches a terminal state has its
  root reaped with its worktree; the root a passing post-merge re-gate was handed is reclaimed by
  `integrate_and_emit`'s post-merge call, so it outlives no landing, the check-in unit's included,
  even when `run_speculation` removed the lane-0 worktree at that path before the re-gate ran, while
  that unit's `cargo-target-<slug>` cache and store-fence sibling stay for its worktree's removal
  or, once it is not live, the backstop; a red re-gate's root stays with its unit (Design); a root
  whose worktree a crash already removed is reclaimed by the next step's backstop, its unit not
  live. Concurrent: the backstop spares every live unit's root by the liveness test it applies to
  the cache sibling; the post-merge call names only its own unit's `rigger-gate-<slug>` and runs
  after `integrate_and_emit` drops its `integrate_mu` guard, so a sibling landing waiting on that
  lock never waits on a reap, and a straggler lens still working the unit keeps its warm cache; a
  speculating unit's lanes have all finished gating when its winner reaches `integrate_and_emit`,
  so that call reclaims lane 0's root while no gate pass of the unit runs, whether lane 0's
  worktree was removed or still stands (Design); and a process still rooted in a root is reaped
  before it is removed (`reap_then_remove_dir`, `reap_dir_before_removal`). Crash-resume: a step
  that dies between removing a worktree and its root, or between a passing post-merge re-gate and
  its reclaim, leaves the root to the resumed step's reclaim or teardown, or to the next step's
  backstop once its unit is not live; the resumed step's re-gate replays the recorded verdicts and
  its reclaim runs on that replayed pass as on a fresh one. Cold start:
  pure over directory names and the run's units. Existing data: a `cargo-mutants-<slug>` directory
  left by an earlier binary matches no arm and is never reclaimed (Notes).
- *Criterion 5.* Empty: a project with no marker gets the no-set workflow and line, every set parsed
  first, so a set that fails `parse_template_set` refuses there too; that workflow's fan-out units
  carry no gate and run under `assert_no_ungated_fanout_unit`'s authored-ungated carve-out, and they
  and its check-in unit are reviewed by its `defaults.review` as a gated unit is (Design); a set
  whose `files` is empty writes no file; a set whose `detect` is empty never matches; a set whose
  `gates` text is empty or comment-only declares no gate, renders `gates: {}` and refuses unless
  both its lists are `[]`; a non-directory entry under `scaffold/` is skipped; a tree with no
  `scaffold/`, or a `scaffold/` holding no set directory, fails the build naming `scaffold/`, so the
  no-set line always lists a set. Repeated: a rerun writes nothing and reports already initialized.
  Reverted: a set file the project deleted is written again on the next run. DROPPED: a file later
  removed from a set's `files` is no longer written and copies already in projects are left alone; a
  project whose marker was removed matches no set and keeps its workflow; a listed file removed from
  this repository fails the build naming it, and a set directory that loses its `set.yml` or `files`
  fails the build naming the set. Concurrent: two inits racing in one project behave as today's
  `write_if_absent`; out of scope. Crash-resume: each file is written on its own, so a rerun
  completes what an interrupted run left. Cold start: pure over the project root's files and the
  embedded sets, and `build.rs` re-runs when any path `generate_template_sets` returned changes,
  `scaffold/` included, so an added set is embedded and a `scaffold/` emptied of sets fails the
  build; an embedded set that fails `parse_template_set` refuses every init, naming its key and
  writing nothing, and fails this repository's scaffold test before it can ship. Existing data: a
  project with a workflow keeps it, gets no gate added and no no-set line, and gets the matched
  set's absent files, an earlier binary's `mutation.sh` kept as every present file is; this
  repository already holds every Rust-set file, so `rigger setup` here writes none; this
  repository's adjudicator persona carries the new adjudicator entry beside its own `boundary` line,
  so the kept repository check holds once the entry changes (Design).
- *Criterion 6.* Empty: a unit branch with no commit since the run branch passes, and a repository
  with no merge base passes with the script's message; a branch that touches only build scripts has
  no source commit and passes. Repeated, reverted, concurrent: a function of the branch's history
  only; a branch rebuilt with its test commit first passes. DROPPED: a commit that deletes a file
  under `src/` touches source, as today. Crash-resume: out of scope - one script run. Cold start:
  reads git only. Existing data: a project scaffolded by an earlier binary keeps its placeholder
  gate, since the workflow is never rewritten (Notes); this repository's own `red-before-green` gate
  runs the new rule on every later unit branch, where every file under `src/` or
  `crates/<name>/src/` is source under both rules; a consumer's non-Rust file under a `src/`
  component, documentation included, is source by design, and a consumer that wants otherwise edits
  its own copy of the script (Design).
- *Criterion 7.* Empty: a homeless environment, or a cache home with no `rigger-mutants` directory,
  changes nothing, since nothing reads either now. Repeated: `rigger result` recorded twice for one
  spawn reclaims its agent scratch once and no-ops after, as today. Reverted: a unit resumed after
  escalation reaches its teardown again and reaps only its worktree, branch and unit siblings.
  DROPPED: with the root deleted, `rigger result` reclaims only the spawn's agent scratch,
  `rigger validate` measures only `agent-scratch` as registered scratch, and
  `rigger reset --build-cache` reclaims no cache-home leaf. Concurrent: no process removes anything
  under the cache home any more, so the speculation-lane races the deleted reclaim guarded against
  are gone, and the agent-scratch reclaim stays keyed on the full spawn id. Crash-resume: decided in
  Design - no emitted event, fold or replay key changes, so a resumed step behaves as every other.
  Cold start: the deleted code resolved the cache home from the environment on each call and kept
  nothing in memory; nothing replaces it. Existing data: an event log written by an earlier binary
  replays unchanged, since no event payload names the root; an operator machine's
  `<cache home>/rigger-mutants`, empty or holding leaves, is never read, measured or removed again
  (Notes).
- *Criterion 8.* Empty: a file with no banned token reports nothing, and a `.rs` file with no
  string literal spanning lines holds no YAML-key line. Repeated: a line holding a token twice, or
  a token and a form, reports once. Reverted: a later change that brings a token back fails the
  rule. DROPPED: a token removed from a line stops being reported. Concurrent, crash-resume: out of
  scope - a pure scan plus text edits. Cold start: pure. Existing data: a comment that names a gate
  with no banned token and no deleted symbol stays as prose (OUT OF SCOPE), the
  `build.mutation` spellings match no form, and an untracked or ignored file under `src/` or
  `crates/` is scanned, as rules 2 and 3 scan one, and one holding a token is red, naming its path
  (Design).
- *Criterion 9.* Out of scope for every corner - it runs the lanes over the integrated result.
- *Global constraints, every criterion.* Hyphens and ASCII: the `scaffold/` files are added lines
  too. No new event type: no criterion emits or folds one. No new crate dependency: the build script
  reads `files` with the standard library, and `serde_yaml` is already a dependency of the root
  package. Both lanes: the resolver, the template sets and rule 4 compile and run in both. Operator
  binary: no unit installs or replaces it, and no test needs it. This repository's pinned
  definition: untouched by every unit, as UNIT ORDER AND BASE decides.

**OUT OF SCOPE.** A language flag for `rigger init` (one set exists, and detection picks it);
per-language persona seeds (`rust-engineer` stays the scaffold's implement and check-in agent); the
core's cargo build environment (`CARGO_TARGET_DIR`, `BuildEnv`), which is not a gate; `boundary` and
`audit` templates; prose that names a gate. That prose - the built-in instruction files (whose
mutation-testing prose names the mutation gate), persona seeds, comments and the handbook
(`boundary`, `audit`, `red-before-green`, `fmt`, `lint`) - stays: THE TEMPLATE SETS' ban on naming
a gate in code never reaches prose, and THE CORE NAMES NO GATE is rule 4, which bans the one gate
the core knew and its tool by token and so reaches prose only on a line holding one of its tokens.
No built-in instruction file or persona seed holds one, and a comment that does is rewritten (THE
CORE NAMES NO GATE).

## Notes (non-criteria)

Type shapes:

```rust
// crates/rigger-domain/src/config.rs
pub struct Gate {
    pub run: String,
    pub kind: String,
    pub inputs: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
}

// crates/rigger-gates-shell/src/gate.rs
pub const GATE_SCRATCH_ENV: &str = "RIGGER_GATE_SCRATCH";
pub struct ResolvedRequirement { pub name: String, pub at: std::path::PathBuf }
pub struct GateRequirements { pub gate: String, pub requires: Vec<ResolvedRequirement> }
pub fn resolve_requirements(
    gates: &BTreeMap<String, rigger_domain::config::Gate>,
    path_var: &std::ffi::OsStr,
) -> Result<Vec<GateRequirements>, RequirementUnavailable>;
pub fn resolve_requirements_on_path(
    gates: &BTreeMap<String, rigger_domain::config::Gate>,
) -> Result<Vec<GateRequirements>, RequirementUnavailable>;
#[error("gate {gate:?} requires {requirement:?}, which is not an executable on PATH (config key: gates.{gate}.requires)")]
pub struct RequirementUnavailable { pub gate: String, pub requirement: String }

// crates/rigger-config-files/src/config_store.rs
pub struct LoadedConfig { pub config: Config, pub gate_requirements: Vec<GateRequirements> }
pub fn load_with_gate_requirements(dir: &str) -> Result<LoadedConfig, Error>;
pub fn load(dir: &str) -> Result<Config, Error>; // load_with_gate_requirements(dir)'s config
impl Config { pub fn validate(&self) -> Result<Vec<GateRequirements>, Error>; }

// src/cli/validate.rs
fn gate_requirement_lines(gates: &[GateRequirements]) -> Vec<String>;

// crates/rigger-domain/src/spawn.rs
pub const UNIT_GATE_SCRATCH_PREFIX: &str = "rigger-gate-";
pub fn unit_scratch_slug(name: &str) -> Option<&str>;

// src/cli/setup.rs
struct TemplateSet { key: &'static str, set: &'static str, files: &'static [(&'static str, &'static str)] }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetSpec { detect: Vec<String>, gates: String, implement: Vec<String>, checkin: Vec<String> }
fn parse_template_set(set: &TemplateSet) -> Result<SetSpec, String>;
fn scaffold_workflow(set: Option<&SetSpec>) -> String;

// build/template_sets.rs
pub struct GeneratedSets { pub source: String, pub watch_paths: Vec<std::path::PathBuf> }
pub fn generate_template_sets(root: &std::path::Path) -> Result<GeneratedSets, String>;
```

The refusal and the validate lines:

```
gate "sweep" requires "cargo-mutants", which is not an executable on PATH (config key: gates.sweep.requires)
gate build: requires nothing
gate sweep: requires cargo-mutants at /home/u/.cargo/bin/cargo-mutants
```

The init lines: with a set, `scaffolded .rigger/workflow.yml (gate template set: rust)`; with none,
the line below, whose parenthetical lists each set as `<key>: <markers joined by " or ">`, sets
joined by `; `:

```
no gate template set matches this project (rust: Cargo.toml at the project root), so .rigger/workflow.yml declares no gates - declare your own under gates:
```

A refusal names the set: `gate template set <key>: <error>`, the error being the parse error or
`<stage> lists gate <id>, which its gates text does not declare`.

Scaffold text: the head comment's `Replace the gate commands with your own.` becomes `Its gates come
from the gate template set matching this project's language; with none, it declares no gates.`; the
check-in comment loses its `then sweeps mutants` clause; the check-in coverage becomes
`the whole spec diff passes every gate on the merged tree`; the implement stage's trailing
`# red -> green enforced around the change` is dropped, since a project with no set lists no gate.

Handbook text, `docs/handbook/authoring-loops.md`. Criterion 1, after the workflow example:

```
A gate may declare `requires:`, the executables its command runs (`requires: [cargo-mutants, cargo-nextest]`). Every validating load (`config_store::load`, and `config_store::load_with_gate_requirements`, which it wraps and `rigger validate` calls) refuses a declared gate whose requirement is not an executable on `PATH`, and `rigger validate` prints one `gate <id>:` line per declared gate naming where each requirement resolved. `config_store::load_workflow`, and `config_store::read_scratch_defaults`, through which `rigger status` and `rigger watch` read their `defaults:`, never validate, so neither checks a requirement.
```

Criterion 3, in the per-unit lifecycle subsection:

```
Every gate that runs for a unit gets `RIGGER_GATE_SCRATCH`, a directory beside the unit's worktree that the unit's gates share and no other unit's gate sees; a gate creates it when it needs one and touches only the names it created there.
```

Criterion 5, in the check-in mutation subsection:

```
A `.rigger/gates/mutation.sh` an earlier `rigger init` wrote reads `$MUTANTS`, which rigger no longer sets, so it fails once wired: delete it and rerun `rigger init`, which writes the current script beside a root `Cargo.toml`.
```

Criterion 5, checklist step 2:

```
2. Check the gates in `.rigger/workflow.yml`: `rigger init` takes them from the gate template set matching your language, and declares none when no set matches - declare your own under `gates:`. The gates must be the same checks CI runs, or the loop green-lights what CI rejects.
```

`scaffold/rust/set.yml`, byte for byte between the fence lines:

```yaml
# The Rust gate template set: rigger init applies it when Cargo.toml is at the project root.
detect: [Cargo.toml]
gates: |
  fmt: { run: "cargo fmt --all --check", kind: core }
  build: { run: "cargo build --workspace", kind: core }
  test: { run: "cargo test --workspace", kind: core }
  lint: { run: "cargo clippy --workspace --all-targets -- -D warnings", kind: elevated }
  # The red-before-green gate: TDD made mechanical. A unit's first source commit must be
  # preceded by, or carry, a test change (.rigger/gates/red-before-green.sh).
  red-before-green: { run: "sh .rigger/gates/red-before-green.sh", kind: core }
  # The check-in mutation sweep ships unwired as .rigger/gates/mutation.sh (its header explains
  # each clause). To run it, declare it and list it after test in the checkin stage's gates
  # (checkin: [fmt, build, test, lint, mutation]):
  #   mutation: { run: "sh .rigger/gates/mutation.sh", kind: core, requires: [cargo-mutants, cargo-nextest] }
  # Each requires entry is an executable on PATH: cargo subcommands that cargo install adds live in
  # $CARGO_HOME/bin, which must be on PATH for requires to see them.
implement: [fmt, build, test, lint, red-before-green]
checkin: [fmt, build, test, lint]
```

`scaffold/rust/files`, byte for byte between the fence lines:

```
.rigger/gates/red-before-green.sh
.rigger/gates/mutation.sh
.rigger/gates/container-env.sh
```

LIVE-WORKFLOW TESTS (criterion 2):

| Test (current name) | File | Disposition |
|---|---|---|
| `rigger_workflow_yml_pins_the_checkin_stage_and_mutation_gate_definition_to_spec_91` | `tests/cli.rs` | deleted |
| `rigger_workflow_yml_wires_the_checkin_stage_and_mutation_gate_with_the_spec_91_shape` | `tests/cli.rs` | deleted; the check-in shape is the conductor fixture test's and the scaffold's |
| `the_checkin_content_gates_diff_the_whole_spec_from_the_run_base_while_unit_gates_keep_the_run_branch` | `tests/cli.rs` | re-homed as `the_content_script_checks_the_whole_spec_from_a_base_and_only_the_branch_without_one`: the same fixture repository, running `sh .rigger/gates/content.sh <check> "$RIGGER_RUN_BASE"` and `sh .rigger/gates/content.sh <check>` for `style` and `no-os-kill` |
| `ci_and_the_lanes_gate_run_one_script_that_derives_its_members` | `tests/ci_lanes.rs` | deleted; its CI half is `ci_runs_the_no_default_and_core_lanes_through_the_lanes_script`; the module doc and the docs of `LANES_SCRIPT` and that test, which name the check-in stage's `lanes` gate, and the `build-test` job's comment in `.github/workflows/rust.yml` that calls `.rigger/gates/lanes.sh` "the script the check-in stage's `lanes` gate runs too", then say CI runs the lanes script and name no check-in gate |
| `this_repository_wires_every_principle_gate_on_the_stages_it_guards` | `tests/principle_gates_wiring.rs` | deleted, with the repository mode of `missing_principle_gates` and `PrincipleGate::repo_command`; the module doc names the scaffold as the one workflow the file reads |
| `the_fmt_clippy_and_build_gates_cover_the_workspace` | `tests/principle_gates_wiring.rs` | deleted: it reads this repository's commands; the Rust set's workspace-wide commands are the text of `scaffold/rust/set.yml` (Notes) |
| `the_test_gate_covers_the_workspace_with_the_container_runtime` | `tests/principle_gates_wiring.rs` | deleted |
| `the_test_gate_runs_the_tests_in_a_worktree_that_predates_the_snippet`, `the_test_gate_fails_when_its_snippet_fails_to_source`, helper `run_test_gate` | `tests/principle_gates_wiring.rs` | deleted: they run this repository's inline `test` command |
| `the_audit_gate_passes_a_fresh_committed_catalog_silently`, `the_audit_gate_names_a_regenerated_uncommitted_catalog_without_failing_on_drift_alone`, `the_audit_gate_fails_red_assertions_with_its_own_diagnostic`, helper `run_audit_gate` | `tests/principle_gates_wiring.rs` | deleted: they run this repository's inline `audit` command |
| helper `repo_gate_command` | `tests/principle_gates_wiring.rs` | deleted |
| `the_mutation_gate_runs_the_shipped_script_and_init_writes_the_same_script` | `tests/principle_gates_wiring.rs` | its first assertion, which reads this repository's `mutation` gate command, deleted |
| `project_events_reads_this_projects_own_real_workflow_yml` | `crates/rigger-grounder/src/grounder/workflowdef.rs` | its gate assertion deleted; the stage and agent assertions stay |

OTHER TEST DISPOSITIONS:

| Test (current name) | File | Disposition | Criterion |
|---|---|---|---|
| `validate_fails_before_any_output_when_the_mutation_gate_has_no_cargo_mutants`, `validate_fails_at_run_start_when_the_scaffolded_mutation_gate_has_no_cargo_mutants_on_path` | `tests/cli.rs` | re-homed as `validate_refuses_before_any_output_when_a_gate_requirement_is_not_on_path` on a fixture gate `sweep` | 1 |
| `validate_reports_mutation_gate_declared_when_cargo_mutants_is_resolvable` | `tests/cli.rs` | re-homed as `validate_reports_each_gate_requirement_resolved_on_path` | 1 |
| `validate_reports_mutation_gate_declared_by_default_on_a_fresh_scaffold`, helper `path_with_no_cargo_mutants` | `tests/cli.rs` | deleted; `path_with_no_known_wrapper` stops staging a fake `cargo-mutants` | 1 |
| `validate_never_probes_for_cargo_mutants_when_no_mutation_gate_is_declared`, `validate_accepts_a_declared_mutation_gate_when_cargo_mutants_is_on_the_real_path` | `crates/rigger-config-files/src/config_store.rs` | replaced by `validate_refuses_the_first_missing_gate_requirement` and `validate_accepts_a_gate_that_requires_nothing` on a synthetic workflow whose gate id is `sweep` | 1 |
| `mutation_gate_binary_available_finds_the_binary_on_path`, `mutation_gate_binary_available_errors_naming_the_binary_and_gate_id_when_absent`, `mutation_gate_binary_available_ignores_a_same_named_non_executable_file_on_path`, `mutation_gate_binary_on_path_reads_the_real_ambient_path` | `crates/rigger-gates-shell/src/gate.rs` | replaced by `resolve_requirements` tests on a synthetic `PATH`: found, missing, a same-named non-executable file, an empty name, a name holding `/`, a symlink to an executable stub (resolved at the symlink's path), a dangling symlink (missing), and an empty and a relative `PATH` component (skipped, so a stub reachable only through either resolves missing); beside them a `path_has_executable` case: a wrapper stub reachable only through a relative `PATH` component is not found | 1 |
| `build_environment_report_reports_mutation_gate_declared`, `build_environment_report_reports_mutation_gate_not_configured` | `src/cli/validate.rs` | replaced by `gate_requirement_lines` tests: a gate requiring nothing, and a gate requiring two executables rendered `<name> at <path>` joined by `, `, in gate-id order | 1 |
| `two_units_gate_environments_never_share_a_mutants_root` | `crates/rigger-conductor/src/conductor.rs` | renamed `two_units_gate_environments_never_share_a_gate_scratch_root` | 3 |
| `an_implement_stage_gate_round_creates_no_mutants_directory` | `crates/rigger-conductor/src/conductor.rs` | renamed `a_gate_round_never_creates_the_gate_scratch_root`, reading `$RIGGER_GATE_SCRATCH` | 3 |
| (new) `exec_runner_removes_an_inherited_gate_scratch_root_when_handed_none` | `crates/rigger-gates-shell/src/gate.rs` | with `RIGGER_GATE_SCRATCH` set in the test's own process, `ExecRunner::run` handed an empty `gate_scratch` runs a command that finds the variable unset (`${RIGGER_GATE_SCRATCH+set}` expands empty), and handed a path runs one that finds that path | 3 |
| `the_post_merge_re_gate_runs_in_its_own_scratch_worktree_never_the_repo` | `crates/rigger-conductor/src/conductor.rs` | its gate logs `$RIGGER_GATE_SCRATCH` in place of `$MUTANTS`, and its root assertion compares that logged path with each unit's `rigger-gate-<slug>` sibling | 3 |
| `run_gates_derives_and_injects_the_review_worktrees_store_fence` | `crates/rigger-conductor/src/conductor.rs` | gains an assertion that the one `gate_scratches` entry its standalone review stage's gate pass (`run_fan_out_review_loop`) records is empty | 3 |
| `a_deferred_gate_runs_once_at_the_phase_boundary_not_inline` | `crates/rigger-conductor/src/conductor.rs` | gains an assertion that the `gate_scratches` entry at the deferred gate's position in `calls` (`run_deferred_gates`) is empty | 3 |
| `unit_mutants_sibling_maps_a_unit_worktree_to_its_mutants_root_and_ignores_the_rest` | `crates/rigger-worktree-git/src/worktree.rs` | renamed `unit_gate_scratch_sibling_maps_a_unit_worktree_to_its_gate_scratch_root_and_ignores_the_rest`, on `UNIT_GATE_SCRATCH_PREFIX` | 3 |
| the `MUTANTS` environment and `cargo-mutants-checkin` directories | `tests/checkin_mutation_diff_base_periphery.rs` | `RIGGER_GATE_SCRATCH` and `rigger-gate-checkin`, plus a run whose `RIGGER_GATE_SCRATCH` is empty, which fails naming `RIGGER_GATE_SCRATCH` and leaves no `unit.diff` in the fixture repository | 3 |
| `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas` | `src/cli/mod.rs` | gains a live and a dead unit's `rigger-gate-<slug>`: the dead one reclaimed and counted, the live one spared | 4 |
| `scan_residue_reports_dead_worktrees_caches_shadows_and_branches` | `src/cli/mod.rs` | gains a dead unit's `rigger-gate-<slug>`, reported among the caches with its size, and a live unit's, omitted | 4 |
| `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share`, for `scratch_totals` | `src/cli/mod.rs` | gains a live and a dead `rigger-gate-<slug>`, both in the per-unit caches total | 4 |
| `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share`, for `scratch_footprint`'s dead filter | `src/cli/mod.rs` | the per-unit caches dead share and reclaimable list hold the dead `rigger-gate-<slug>`, never the live one | 4 |
| `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches` | `src/cli/mod.rs` | gains an `events.db` inside a `rigger-gate-<slug>`, pruned | 4 |
| `worktree_remove_also_reclaims_the_sibling_mutants_root` | `crates/rigger-worktree-git/src/worktree.rs` | renamed `worktree_remove_also_reclaims_the_sibling_gate_scratch_root`, on `UNIT_GATE_SCRATCH_PREFIX` | 4 |
| `worktree_remove_reaps_a_process_rooted_in_its_sibling_mutants_root_before_reclaiming_it` | `tests/reap_before_removal_periphery.rs` | renamed `..._sibling_gate_scratch_root_...`, on `UNIT_GATE_SCRATCH_PREFIX` | 4 |
| (new) `a_passing_post_merge_re_gate_reclaims_the_gate_scratch_root_its_fallback_names` | `crates/rigger-conductor/src/conductor.rs` | a speculating unit whose lane 0 implementer crashes and whose lane 1 wins, merging into a run branch another unit moved so its post-merge re-gate misses the content-addressed replay and runs its gate, which creates `$RIGGER_GATE_SCRATCH` and `$CARGO_TARGET_DIR`: once the unit integrates, the `rigger-gate-<slug>` sibling of `unit_worktree_dir`'s path, the root only that re-gate is handed, does not exist, and the `cargo-target-<slug>` sibling the same gate created still does, since the post-merge reclaim leaves the cache to the worktree's removal; and a replayed pass reclaims too: a step resumed over a seeded log holding the unit's `integrate-landed` row and a passing verdict under its `GateKey::PostMergeVerdict` key, as `a_resumed_landed_but_ungated_unit_regates_the_landed_tree` seeds its landed row, with that `rigger-gate-<slug>` sibling present on disk, replays the verdict with no gate run recorded and leaves the sibling absent | 4 |
| `scaffold_parses_into_a_valid_config` | `src/cli/setup.rs` | names no gate id: every embedded set parses through `parse_template_set`, every rendering (each set's and the no-set one) loads and validates, no rendered gate command is a placeholder, and every id a stage list names is declared; its gate-count, `checkin` gate-list and `mutation` gate assertions are dropped, and its agent, stage-shape, review, grounder and budget assertions hold on every rendering | 5 |
| (new) `parse_template_set_refuses_naming_the_set_key` | `src/cli/setup.rs` | a synthetic set whose `set.yml` fails to parse, one whose `gates` text fails to parse and one whose `checkin` lists an undeclared gate id each refuse naming the set key | 5 |
| (new) `scaffold_workflow_renders_a_comment_only_gates_text_as_an_empty_mapping` | `src/cli/setup.rs` | a synthetic set whose `gates` text holds only a comment and whose lists are `[]` parses, renders `gates: {}` and loads | 5 |
| (new) `init_project_matching_no_set_writes_a_gateless_workflow_and_the_no_set_line` | `src/cli/setup.rs` | `init_project` beside no marker reports no `gate_set`, writes a workflow that loads with no gate and `[]` on `implement` and `checkin`, and `scaffold_summary_lines` holds the no-set line | 5 |
| (new) `tests/template_sets_build.rs` | `tests/` | refuses an absolute path, a `..` segment and a missing file, naming the set and the path, a set directory missing `set.yml` or missing `files`, naming the set, and a root with no `scaffold/` and a `scaffold/` holding no set directory, naming `scaffold/`; skips a file directly under `scaffold/`; returns `scaffold/`, both files of every set and every listed file as watch paths | 5 |
| `a_scaffolded_consumer_project_carries_every_principle_gate_and_checklist_line` | `tests/principle_gates_wiring.rs` | renamed `a_scaffolded_rust_project_carries_the_rust_sets_gates_and_every_checklist_line`, criterion 5's init test: `rigger init` in a fixture with a root `Cargo.toml` writes a workflow that loads through `config_store::load`, declares exactly `fmt`, `build`, `test`, `lint` and `red-before-green`, and lists `[fmt, build, test, lint, red-before-green]` on `implement` and `[fmt, build, test, lint]` on `checkin`, the one pin of the Rust set's gate ids and stage lists, and the scaffolded personas carry every `PERSONA_CHECKLIST` line, its adjudicator entry being the scaffold's new line; `missing_principle_gates`, `PRINCIPLE_GATES` and `PrincipleGate`, which those exact lists subsume, are deleted | 5 |
| `the_mutation_gate_runs_the_shipped_script_and_init_writes_the_same_script`, after the first assertion criterion 2 deletes | `tests/principle_gates_wiring.rs` | replaced by `init_writes_each_file_the_rust_set_lists`: `rigger init` in a fixture with a root `Cargo.toml` writes every path `scaffold/rust/files` lists, each with the bytes of this repository's copy | 5 |
| `every_persona_carries_its_principle_gate_checklist_line` | `tests/principle_gates_wiring.rs` | kept whole: with the adjudicator entry changed it asserts the gate-agnostic sentence on this repository's adjudicator persona, which carries it beside its `boundary` sentence | 5 |
| (new) `red_before_green_fails_a_test_less_commit_to_a_member_outside_crates` | `tests/principle_gates_wiring.rs` | a case beside the red-before-green cases: a test-less commit to `tools/x/src/lib.rs` fails, naming the commit; the unit's red, beside the scaffolded-command behaviour pin (Design) | 6 |
| `footprint_report_folds_a_none_mutation_root_to_a_zero_contribution`; every `mutation_scratch_*` and `reclaim_unit_mutation_scratch_*` test in `replay.rs`; `tests/mutation_scratch_root_periphery.rs` and `tests/mutation_scratch_reap_base_guard_periphery.rs` whole; in `tests/spawn_scratch_reap_authorized_root_periphery.rs` the three tests whose names hold `mutation_scratch_dir`; in `tests/cli.rs` `populated_mutation_scratch`, `registered_mutation_scratch_root`, `assert_a_speculation_exit_reaps_every_lanes_mutation_scratch` and every test whose name holds `mutation_scratch` | several | deleted | 7 |
| `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots`, `a_leading_slash_spawn_id_never_collapses_the_reclaim_to_its_registered_root`, helper `assert_a_hostile_spawn_id_spares_its_neighbours`, `validate_reports_footprint_by_category_and_flags_a_dead_share_breach`, `validate_flags_registered_scratch_roots_dead_share_scoped_to_real_spawn_liveness_in_the_store`, `validate_flags_a_prior_abandoned_runs_orphan_even_when_a_later_run_reuses_the_identical_spawn_id` (`tests/cli.rs`); `footprint_report_measures_every_category_on_a_seeded_fixture_tree`, `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn`, `footprint_advisories_name_reset_build_cache_for_every_class_it_reclaims` (`src/cli/mod.rs`); `spawn_scratch_path_and_mutation_scratch_path_hex_escape_a_dotdot_id_so_it_can_never_escape` (`replay.rs`) | several | the cache-home half dropped, every other assertion kept | 7 |
| `footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone`, `footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket` | `src/cli/mod.rs` | the `mutation_root` argument dropped from each `footprint_report` call, every assertion kept | 7 |
| `reset_build_cache_reclaims_every_dead_class_validate_accounts_and_spares_a_held_dir` | `tests/reset_build_cache_periphery.rs` | its dead mutation-scratch leaf, that leaf's assertion, its `mutation_scratch_root` call and the cache-home binding only that call reads dropped; every other assertion kept, the mutation anchor's survival among them | 7 |
| `render_section_3`, the tier 1 plan item it feeds, and the assertions citing either | `tests/simplification_audit.rs` | the violation whose subject is `reclaim_terminal_unit_mutation_scratch` and its relocation item dropped; the regenerated `docs/audit/` files committed | 7 |
| (new) rule 4 | `tests/boundary_audit.rs` | its fixture file holds one line of each token and form, its `MUTANTS` line spelled `$MUTANTS`, each reported by file and line, beside a `BuildConfig` literal's `mutation:` field line, an inline `"build:\n  mutation: on\n"` string and a line naming `UNIT_MUTANTS_PREFIX`, none reported | 8 |
| `every_reviewer_prompt_forbids_cargo_mutants` | `crates/rigger-conductor/src/conductor.rs` | renamed `every_reviewer_prompt_forbids_a_mutation_sweep` | 8 |
| the persona pins named in THE CORE NAMES NO GATE | `src/cli/mod.rs` | moved to `tests/principle_gates_wiring.rs` | 8 |
| `stages_gates_and_agents_all_become_concepts`, `needs_and_runs_edges_match_the_yaml_lists` (`workflowdef.rs`); `workflow_definition_events_fold_into_stage_gate_agent_nodes_with_needs_runs_reviews_edges` (`sqlite.rs`) | several | the fixture gate `sweep` and the concept `gate:sweep` | 8 |
| `is_reapable_base_accepts_a_root_that_is_not_named_dot_rigger_tmp_at_all` | `crates/rigger-process/src/reap.rs` | its fixture root renamed from `rigger-mutants`; the assertion kept | 8 |

Deferrals and leftovers:
- Re-wiring the mutation gate into this repository's check-in stage (issue #32) is OUT, and so is
  giving that gate `requires: [cargo-mutants, cargo-nextest]`: the requirement check applies to
  every declared gate, so the key would make every load of this repository's configuration need both
  executables on `PATH`, the three retained tests' validating loads in CI among them, where
  `.github/workflows/rust.yml` then installs neither: that need is the environment the Goal's claim
  leaves out (TESTS PIN FIXTURES AND THE SCAFFOLD). The re-wiring change adds the key and installs
  both in CI together; until then the gate declares no `requires`. The operator's edits to the
  pinned `.rigger/workflow.yml`, made outside a run, are two comments: the one above
  the `mutation` gate, which still says declaring the id requires `cargo-mutants`, and the one above
  the `red-before-green` gate, which still names the old source rule (`src/` or
  `crates/<name>/src/`).
- This repository's adjudicator persona carries both its own `boundary` sentence, which names a
  gate this repository wires, and the gate-agnostic sentence that becomes `PERSONA_CHECKLIST`'s
  adjudicator entry; the persona set is the pinned definition, and no unit edits it.
- A `cargo-mutants-<slug>` directory under a scratch root and an empty `<cache home>/rigger-mutants`
  directory left by an earlier binary are the operator's to delete; nothing reclaims them.
- A `.rigger/gates/mutation.sh` an earlier binary scaffolded into a consumer project is the
  consumer's to replace: delete it and rerun `rigger init` (Design).
- A consumer project scaffolded by an earlier binary keeps its placeholder gates; replacing them is
  the consumer's edit.

## Global constraints

- Hyphens, never em or en dashes, and ASCII only, in every added line.
- No new event type; no new crate dependency.
- Both feature lanes green (fmt, clippy -D warnings, test on default and --no-default-features).
- The operator's installed rigger binary is never replaced or modified by any unit.
- No unit edits this repository's `.rigger/workflow.yml`, `.rigger/agents/` or `.rigger/instructions/`.
- Fewest moving parts: one requirement resolver run once per load, one gate scratch root, one
  scratch-name predicate, one template-set embedding, one template-set parser and one source lexer
  for the source audits.
- A criterion whose test cannot be written as specified escalates naming the blocking fact; no unit
  narrows a fixture, re-introduces a gate name or keeps a live-workflow read to get green.
- Flagged for the adjudicator, which the gates cannot see: each row of LIVE-WORKFLOW TESTS is shown
  deleted or re-homed in the diff, each comment criterion 8 rewords names the generic mechanism,
  `.github/workflows/rust.yml` installs no `cargo-mutants` and that install step's comment gives
  only the `sccache` reason, and no rule-4 token or form is split across literals or built at run
  time to pass the rule.

## Done when

- [ ] a test proves GATE REQUIREMENTS RESOLVE THROUGH ONE PATH: `rigger validate` prints `gate sweep: requires <name> at <path>` for a fixture gate whose `requires` names a stub executable first on `PATH`,
  and without that stub refuses before any output with the one requirement message naming the gate,
  the executable and `gates.sweep.requires`, while a fixture gate named `mutation` that declares
  nothing validates with no `cargo-mutants` on `PATH`, asserted in `tests/cli.rs`. This criterion
  OWNS the `requires` key, the resolver, the refusal, the per-gate validate lines, the removal of
  the mutation probe, `MUTATION_GATE_ID` and the mutation validate line, the
  `.github/workflows/rust.yml` edit and its handbook paragraph; the core-wide token rule is
  criterion 8's, NOT this one's.
- [ ] a test proves CHECK-IN GATES RUN ONLY AT CHECK-IN, ON A FIXTURE: a `RecordingRunner` records no run of the gate only the fixture's check-in stage lists before the last run of the gate only its fan-out implement stage lists,
  and records as many check-in-only runs for a three-criterion spec as for a two-criterion one,
  asserted in `crates/rigger-conductor/src/conductor.rs` beside
  `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates`. This
  criterion OWNS that test and every disposition in the Notes table LIVE-WORKFLOW TESTS; the
  scaffold's own pins are criterion 5's, NOT this one's.
- [ ] a test proves THE GATE SCRATCH ROOT IS HANDED GENERICALLY: a gate run in a unit worktree is handed `RIGGER_GATE_SCRATCH` naming that worktree's `rigger-gate-<slug>` sibling, distinct per unit,
  and unset on a standalone review worktree or a run with no worktree even when the gate's parent
  process holds it, asserted in `two_units_gate_environments_never_share_a_gate_scratch_root`,
  `unit_gate_scratch_sibling_maps_a_unit_worktree_to_its_gate_scratch_root_and_ignores_the_rest`,
  `a_gate_round_never_creates_the_gate_scratch_root` and
  `exec_runner_removes_an_inherited_gate_scratch_root_when_handed_none`. This criterion OWNS the
  variable, `UNIT_GATE_SCRATCH_PREFIX`, the `Runner::run` parameter rename, `RecordingRunner`'s
  field, the post-merge re-gate's fallback, the root line and gate-environment paragraph of
  `.rigger/gates/mutation.sh` and its handbook sentence; the root's reclamation is criterion 4's,
  NOT this one's.
- [ ] a test proves THE GATE SCRATCH ROOT HAS ONE LIFECYCLE: every scratch walk classifies a `rigger-gate-<slug>` as its unit's per-unit cache, reclaiming it when the unit is not live and sparing it when it is,
  and `reclaim_cache_sibling` removes it with the unit's worktree, asserted in
  `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas`,
  `scan_residue_reports_dead_worktrees_caches_shadows_and_branches`,
  `scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share` (for `scratch_totals` and
  `scratch_footprint`'s dead filter), `find_shadow_stores_finds_nested_events_db_and_prunes_build_caches`
  and `worktree_remove_also_reclaims_the_sibling_gate_scratch_root`. This criterion OWNS
  `unit_scratch_slug` at every scratch walk, `reclaim_cache_sibling`'s move to
  `UNIT_GATE_SCRATCH_PREFIX` and the deletion of `UNIT_MUTANTS_PREFIX`; the variable and its prefix
  are criterion 3's, NOT this one's.
- [ ] a test proves INIT SCAFFOLDS GATES FROM THE PROJECT'S TEMPLATE SET: `rigger init` beside a root `Cargo.toml` writes the Rust set's gates, stage gate lists and listed files into a fixture project,
  each listed file with the bytes of this repository's copy, while a project matching no set gets a
  workflow declaring no gates and the no-set line, and no rendering wires a placeholder gate, the
  Rust set's gates, lists and files asserted in `tests/principle_gates_wiring.rs`, and the no-set
  workflow and line and the placeholder ban in `src/cli/setup.rs`. This criterion OWNS
  `scaffold/`, the build-script embedding, the detection, the rendering, the no-set line, the
  scaffolded adjudicator's checklist line, the sentences of `.rigger/gates/mutation.sh` that
  describe the scaffold or a stage, and its handbook text; the red-before-green verdict is criterion
  6's, NOT this one's.
- [ ] a test proves THE SCAFFOLDED RED-BEFORE-GREEN GATE FAILS A TEST-LESS COMMIT IN ANY LAYOUT: in a Rust fixture repository after `rigger init`, the scaffolded `red-before-green` command run under `sh -c` fails a test-less `src/` commit,
  naming that commit, and passes a branch whose test commit comes first, while the shipped script
  fails a test-less commit to `tools/x/src/lib.rs`, asserted in `tests/principle_gates_wiring.rs`.
  This criterion OWNS that test, the layout case and `is_source_path` with its header sentence in
  `.rigger/gates/red-before-green.sh`; the set that wires the gate is criterion 5's, NOT this one's.
- [ ] a test proves THE SPAWN-KEYED CACHE-HOME ROOT IS DELETED: `footprint_report` measures the registered scratch roots category from `agent-scratch` alone and `rigger result` reclaims only the reporting spawn's agent scratch,
  asserted in the assertions OTHER TEST DISPOSITIONS keeps for criterion 7,
  `footprint_report_measures_every_category_on_a_seeded_fixture_tree` and
  `reset_build_cache_reclaims_every_dead_class_validate_accounts_and_spares_a_held_dir` among them.
  This criterion OWNS the deletion of the spawn-keyed cache-home root with every caller and test of
  it, and the simplification audit text that cites it; the core-wide token rule is criterion 8's,
  NOT this one's.
- [ ] a test proves THE CORE NAMES NO GATE: rule 4 of `tests/boundary_audit.rs` finds no line under `src/` or `crates/` holding a banned gate or tool token and reports a fixture file that holds one by file and line.
  This criterion OWNS the rule and the removal of every occurrence criteria 1, 3, 4, 5 and 7 leave;
  each of those criteria's own removals is theirs, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
