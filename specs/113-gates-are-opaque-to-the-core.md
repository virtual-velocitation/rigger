# 113 - Gates are opaque to the core

**Goal:** rigger knows no gate by name. A gate is a command a project's `.rigger/workflow.yml`
wires into a stage, so adding or removing one changes no Rust source and no test (issue #33).
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

**UNIT ORDER AND BASE, decided here.** Criteria 1, 2 and 3 need nothing. Criterion 4 needs criteria
1 and 2 (its scaffold comment names `requires`, and the principle-gate helper it narrows loses its
live-workflow mode in criterion 2). Criterion 5 needs criterion 4. Criterion 6 needs criteria 1, 3
and 4 (its rule stays red until their removals land). Criterion 7 needs all six. The spec is
launched on rigger-run. No unit edits this repository's `.rigger/workflow.yml`, `.rigger/agents/` or
`.rigger/instructions/`: they are the run's pinned definition (`definition_hash`), and the
operator's installed binary refuses unknown workflow keys, so a `requires` key there would stop the
run that builds it.

**A GATE DECLARES WHAT IT REQUIRES, decided here.** `config::Gate`
(`crates/rigger-domain/src/config.rs`) gains `requires: Vec<String>` under the config key
`gates.<id>.requires`, `#[serde(default)]`: executable names, empty when absent. One resolver in
`crates/rigger-gates-shell/src/gate.rs`, `resolve_requirements(gates, path_var)`, resolves every
entry of every declared gate in gate-id order, then list order, as written (a name listed twice
resolves twice). It looks each entry up as a file name in each `PATH` directory in order through
`find_executable`, which returns the first executable regular file it finds; `path_has_executable`,
the wrapper probe, calls it, so one lookup serves both. An empty entry or one containing `/` names
no file in a `PATH` directory and resolves missing. `resolve_requirements_on_path(gates)` is its
ambient-`PATH` edge. `Config::validate` calls that edge after the retired-key refusals and refuses
on the first missing requirement with the one message shape (Notes), whether or not a stage lists
the gate: a declared gate is a gate the operator wired. A `match` or `if` on a gate id is NOT an
implementation of this check, and neither is a requirement list kept anywhere but the gate's own
`requires`. `MUTATION_BINARY`, `MUTATION_GATE_ID`, `MutationBinaryUnavailable`,
`mutation_gate_binary_available` and `mutation_gate_binary_on_path` are deleted with their tests,
and every comment that describes the deleted probe describes the requirement check instead (the
`config_store` module doc, `read_scratch_defaults`'s doc, the doc of `BuildConfig::mutation`, the
`crates/rigger-config-files/Cargo.toml` comment). The `build.mutation` refusal keeps its key and
spec number, and its reason becomes `spec 91 retired it: nothing reads it`. `SCAFFOLD_WORKFLOW`
loses the sentence claiming a gate under the id `mutation` requires `cargo-mutants`.
`.github/workflows/rust.yml` stops installing `cargo-mutants`, and its comment gives only the
`sccache` reason.

**VALIDATE REPORTS EVERY GATE THROUGH THE SAME RESOLVER, decided here.** After its `build budget:`
line, `rigger validate` prints one line per declared gate in gate-id order, rendered by a pure
`gate_requirement_lines` from `resolve_requirements_on_path`'s answer:
`gate <id>: requires nothing`, or `gate <id>: requires <name> at <path>` with further entries joined
by `, `. A missing requirement never reaches this report: `config_store::load` runs
`Config::validate` first, so the command refuses before any output. `build_environment_report` loses
its third parameter and its mutation line.

**TESTS PIN FIXTURES AND THE SCAFFOLD, NEVER THE LIVE WORKFLOW, decided here.** No test reads this
repository's `.rigger/workflow.yml` gate list, gate commands or stage coverage; each test that does
is deleted or re-homed as the Notes table LIVE-WORKFLOW TESTS says. A test that reads the live
workflow but skips when a gate is missing is still a live-workflow read. The check-in-once behaviour
is asserted on a fixture through the conductor: a fixture workflow whose fan-out `implement`
template lists only the gate `unit-check` and whose `checkin` stage (`needs: [implement]`) lists
only the gate `sweep`, run through `run_isolated` with a `RecordingRunner` that passes every gate,
once on a two-criterion spec and once on a three-criterion spec. A test that reads this repository's
workflow for something other than gates (`shipped_workflows_carry_a_non_zero_spawn_budget`, the
stage and agent assertions of `project_events_reads_this_projects_own_real_workflow_yml`) stays.

**THE GATE SCRATCH ROOT, decided here.** Every gate that runs for a unit is handed
`RIGGER_GATE_SCRATCH` (`GATE_SCRATCH_ENV`, beside `STORE_FENCE_ENV` in
`crates/rigger-gates-shell/src/gate.rs`) naming `<scratch root>/rigger-gate-<slug>`: the
`unit_sibling` of the unit worktree under `UNIT_GATE_SCRATCH_PREFIX` (`"rigger-gate-"`, which
replaces `UNIT_MUTANTS_PREFIX` in `crates/rigger-domain/src/spawn.rs`). `run_gates_at` derives it
where it derives the mutants root today, keeping the post-merge re-gate's fallback to the unit's own
worktree path, and `run_regenerate_command` likewise; a gate on a review worktree, the integrated
tree or a worktree-less run gets the variable unset. The `Runner::run` parameter `mutants_dir`
becomes `gate_scratch` in every implementation (`ExecRunner`, the conductor's test runners,
`ReplayRunner` in `src/cli/mod.rs`, the runners in `tests/integrate_conflict_merge_periphery.rs`),
and `RecordingRunner`'s `mutants_dirs` becomes `gate_scratches`. rigger never creates the root; a
gate that uses it creates it. A per-gate root, or any path or variable keyed by a gate id, is NOT
this root. One predicate, `unit_scratch_slug(name)` in `crates/rigger-domain/src/spawn.rs`, returns
the slug of a `cargo-target-<slug>` or `rigger-gate-<slug>` name, and every scratch walk classifies
by it: `reclaim_orphan_scratch`'s per-unit arm, `scan_residue`, `scratch_totals`,
`scratch_footprint`'s dead filter and `find_shadow_stores`'s prune. A `rigger-gate-<slug>` whose
unit is not live is therefore reported, measured and reclaimed in the per-unit caches category as
its cache sibling is, and a live unit's is spared. `reclaim_cache_sibling` reaps and removes it when
the unit's worktree is removed, as it does the mutants root today. `.rigger/gates/mutation.sh` takes
its root as `MUTANTS=${RIGGER_GATE_SCRATCH-}` at its top, keeps its anchor at
`${MUTANTS%/*}/mutation-anchor` beside the root, and its gate-environment paragraph names
`RIGGER_GATE_SCRATCH` and `unit_sibling`.

**THE TEMPLATE SETS, decided here.** A template set is a directory `scaffold/<key>/` at the
repository root holding two files. `set.yml` declares `detect` (marker file names at the project
root), `gates` (the text of the `gates:` block, comments kept), `implement` and `checkin` (the two
stages' gate lists); `src/cli/setup.rs` parses it with `serde_yaml`, every key required and unknown
keys refused. `files` lists one repository-relative path per line, blank lines skipped; each path is
written to the same path in the scaffolded project, its parent directories created as needed.
`build.rs`, through `build/template_sets.rs` included by `#[path]` as `build/gitsemver.rs` is,
enumerates `scaffold/*/` in name order and writes `$OUT_DIR/template_sets.rs`: a `TEMPLATE_SETS`
array holding one `TemplateSet` per directory with its key, `include_str!` of its `set.yml` and
`include_str!` of each listed file, plus `cargo:rerun-if-changed` for `scaffold/`, both files of
every set and every listed file. A listed path that is absolute, holds a `..` segment or names no
file fails the build naming the set and the path. `build/template_sets.rs` is pure over the
repository root (it returns the generated source or the refusal), and `tests/template_sets_build.rs`
includes it by `#[path]`, as `tests/build_watch_paths.rs` includes `build/watch.rs`.
`src/cli/setup.rs` `include!`s the generated file. The embedded bytes are data `rigger init` copies;
no code in `src/` or `crates/` names a gate or a script. Why files embedded by the build script:
each shipped script keeps one home, its file under this repository's `.rigger/gates/`, where an
embedded copy in Rust text would be a second home; `src/` then names no script, where an
`include_str!` in `src/` would name `mutation.sh`; and the binary stays self-contained, where files
read at run time would need an installed location beside the binary, a moving part `rigger init`
does not have. `set.yml` stays text parsed at run time so the build script needs no parsing crate. A
copy of a script under `scaffold/` or in Rust text is NOT the template set; the listed file is.

The key is the set directory's name. `init_project` picks the first set in key order any of whose
`detect` markers is a file at the project root; there is no language flag. `SCAFFOLD_WORKFLOW`
carries three placeholders - `@GATES@` on its own line where the `gates:` block goes,
`@IMPLEMENT_GATES@` and `@CHECKIN_GATES@` as the two stages' `gates:` values - and one pure
`scaffold_workflow(set)` renders it: with a set, `gates:` followed by the set's `gates` text
indented two spaces and the two lists as YAML flow sequences; with none, `gates: {}` and `[]` for
both lists. Its head comment, its check-in comment and its check-in coverage drop the mutation sweep
for the language-neutral text in Notes. The workflow is written only when absent, as today; the
matched set's files are written when absent whether or not the workflow was. `ScaffoldReport`
replaces `new_gate_files` with `new_set_files` (the paths written) and gains `gate_set` (the matched
key). `scaffold_summary_lines` prints `scaffolded <path>` per written file, names the set on the
workflow line when one applied, and prints the no-set line (Notes) when the workflow was written
with none. `SCAFFOLD_GATE_FILES` is deleted. No rendering wires a placeholder: a scaffold gate whose
command is empty, `true`, `:` or ends in `; true` fails `src/cli/setup.rs`'s scaffold test, and a
project matching no set gets no gate rather than a stand-in. The Rust set (Notes) wires `build`,
`test`, `lint` and `red-before-green`, ships `.rigger/gates/mutation.sh` unwired with the
`.rigger/gates/container-env.sh` it sources, and shows the mutation gate's declaration with its
`requires` in a comment. It scaffolds no `boundary` or `audit` gate: neither has a per-language
implementation, and a stand-in is banned. Every `src/cli/setup.rs` test that reads
`SCAFFOLD_WORKFLOW` reads `scaffold_workflow` output instead, both renderings where it asserts the
workflow loads. A test that needs gates in a scaffolded project declares them in its own fixture
workflow. `docs/handbook/authoring-loops.md`'s check-in mutation section and
`.rigger/gates/mutation.sh`'s first paragraph say the Rust set ships the script unwired and how to
wire it.

**THE RUST SET'S RED-BEFORE-GREEN IS THIS REPOSITORY'S SCRIPT, decided here.** The Rust set's
`files` lists `.rigger/gates/red-before-green.sh` and its `red-before-green` gate runs
`sh .rigger/gates/red-before-green.sh`, so the shipped template is the file this repository's own
`red-before-green` gate runs: one source of truth, nothing generated, no drift check. Its default
run branch, `rigger-run`, is the run branch rigger anchors in every project (`RUN_BRANCH`).

**THE CORE NAMES NO GATE, decided here.** `tests/boundary_audit.rs` gains rule 4: no file under
`src/` or `crates/`, of any extension and read as lossy UTF-8, holds a line containing,
case-sensitively, `cargo-mutants`, `cargo mutants`, `mutation.sh`, `rigger-mutants`, `MUTANTS` as a
whole word, an identifier containing `GATE_ID`, or the string literal `"mutation"`. Violations are
reported by file and line, and the rule carries no allowlist. A token split across `concat!`,
`format!` or adjacent literals to pass the rule is NOT a removal. Criterion 6 removes every
occurrence criteria 1, 3 and 4 leave:

- The spawn-keyed cache-home root, deleted whole: `MUTATION_SCRATCH_SUBDIR`,
  `mutation_scratch_path`, `reclaim_unit_mutation_scratch` and `mutation_scratch_root` in
  `crates/rigger-driver/src/driver/replay.rs`; `reclaim_terminal_unit_mutation_scratch` and
  `mutation_scratch_settled` with their call sites in the conductor; the cache-home half of
  `reclaim_spawn_registered_scratch` in `src/cli/run.rs`; and the `mutation_root` parameter of
  `footprint_report` with its computation in `measure_footprint`, so the registered scratch roots
  category measures `agent-scratch` alone. `cache_home_from` stays: `worktree.rs` and `hygiene.rs`
  read it. A test whose subject is the root is deleted; a test that also asserts agent-scratch,
  footprint, reset or reap behaviour keeps those assertions (Notes).
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
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` tests becomes `sweep`, and its command
  `sh sweep.sh`.
- Every comment under `src/` and `crates/` that names a banned token or a symbol this spec deletes
  names the generic mechanism instead; `tests/simplification_audit.rs`'s report text drops the
  violation whose subject, `reclaim_terminal_unit_mutation_scratch`, is deleted; and
  `docs/architecture-addendum-world-authority.md` names `unit_sibling` where it named
  `mutation_scratch_path`.

**CRITERIA 1 AND 4 SPLIT AT `SCAFFOLD_WORKFLOW`.** Criterion 1 deletes only the sentence claiming a
gate under the id `mutation` requires `cargo-mutants`; criterion 4 owns the rest of the scaffold
text.

**CRITERIA 2 AND 4 SPLIT AT
`the_mutation_gate_runs_the_shipped_script_and_init_writes_the_same_script`.** Criterion 2 deletes
its first assertion, which reads this repository's `mutation` gate command, and the repository mode
of `missing_principle_gates`; criterion 4 replaces the rest of that test with
`init_writes_each_file_the_rust_set_lists` and narrows the scaffold half of the principle-gate
check.

**CRITERIA 3 AND 4 SPLIT AT `.rigger/gates/mutation.sh`.** Criterion 3 owns its root line and its
gate-environment paragraph; criterion 4 owns its first paragraph. Neither asserts the other's text.

**LANDING-ORDER SIMULATION** (ordered pairs sharing a surface; 1, 2 and 3 in any order, 4 after 1
and 2, 5 after 4, 6 after 1, 3 and 4, 7 last):

| First | Without | Shared surface | Does the first one's own text hold? |
|---|---|---|---|
| 1 | 2 | `Config::validate` over this repository's workflow, which a live-workflow test calls | yes - that workflow declares no `requires`, so its validation holds with no `cargo-mutants` on `PATH` |
| 2 | 1 | the same | yes - 2 deletes that test, and its conductor fixture declares no `requires` |
| 1 | 3 | `crates/rigger-gates-shell/src/gate.rs` | yes - 1 changes the probe items and `path_has_executable`, 3 changes `ExecRunner::run` and the `Runner` doc; disjoint items |
| 3 | 1 | the same | yes - the same disjoint items |
| 2 | 3 | the live test asserting the script reads `$MUTANTS` | yes - 2 deletes the test and edits no script |
| 3 | 2 | the same | yes - the script keeps `$MUTANTS` as its own variable, set from `RIGGER_GATE_SCRATCH`, so the live test still finds it until 2 deletes it |
| 1 | 4 | `SCAFFOLD_WORKFLOW`; `rigger validate` on a fresh scaffold | yes - the placeholder `mutation` gate declares no `requires`, so a fresh scaffold validates, and 1 deletes only its stale sentence |
| 4 | 1 | the scaffold comment naming `requires` | not reachable - 4 needs 1 |
| 2 | 4 | `missing_principle_gates`, `PRINCIPLE_GATES`, the mutation-script test | yes - the scaffold mode still finds the placeholder `boundary`, `audit` and `red-before-green` gates on their stages |
| 4 | 2 | the same | not reachable - 4 needs 2 |
| 3 | 4 | `.rigger/gates/mutation.sh` | yes - 3 edits its root line and gate-environment paragraph; init still writes the file from the same path |
| 4 | 3 | the same | yes - 4 edits only the first paragraph, and its init test compares the written file with this repository's file, whatever paragraph 3 later edits |
| 4 | 5 | the scaffolded `red-before-green` gate | yes - 4 asserts the gate is declared, listed and its script written, nothing about its verdict |
| 5 | 4 | the same | not reachable - 5 needs 4 |
| 1, 3, 4 | 6 | rule 4 of `tests/boundary_audit.rs` | yes - none of them asserts the core-wide scan |
| 6 | 1, 3, 4 | the same | not reachable - 6 needs all three |
| 2 | 6 | `tests/principle_gates_wiring.rs` | yes - 2 deletes repository-mode tests, 6 adds moved persona tests; disjoint items |
| 6 | 2 | the same | not reachable - 6 needs 4, which needs 2 |
| 5 | 6 | `tests/principle_gates_wiring.rs` | yes - disjoint tests |
| 6 | 5 | the same | yes - 6 neither reads nor writes the red-before-green tests |

**CONSTRAINTS WALK.**
- *Criterion 1.* Empty: an absent or empty `requires` resolves nothing, validates, and reports
  `requires nothing`; a workflow with no gates prints no gate line; an unset `PATH` resolves every
  requirement missing. Repeated: a name listed twice resolves and prints twice; every `rigger step`
  re-runs the check through `config_store::load`. Reverted: a requirement installed again passes the
  next load. DROPPED: an executable removed from `PATH` mid-run makes the next step's load refuse
  with the message, before any gate runs; a `requires` entry deleted from a gate stops being checked
  at the next load. Concurrent: a read of `PATH` and file metadata only. Crash-resume: nothing is
  written, so a resumed step re-runs the check. Cold start: pure over the workflow and `PATH`.
  Existing data: a workflow with no `requires` key parses unchanged; a workflow that declares a gate
  named `mutation` with no `requires` validates with no `cargo-mutants` anywhere; the
  `build.mutation` refusal still names its key and spec 91.
- *Criterion 2.* Empty: out of scope - the fixture always has criteria. Repeated: two runs of one
  fixture record one order. Reverted, DROPPED: a gate added to or removed from this repository's
  workflow touches no test; that is the property the deletions buy. Concurrent: implement units may
  gate concurrently; the assertions compare stage-exclusive gate ids by recorded order, which holds
  under any interleaving of implement units. Crash-resume: out of scope - an in-process fixture run.
  Cold start: an in-memory store. Existing data: the deleted tests carry no data.
- *Criterion 3.* Empty: a gate with no unit worktree gets the variable unset; a scratch root with no
  `rigger-gate-` entry is walked as today. Repeated: every gate round of a unit is handed the same
  root, whose content the gate owns. Reverted: a unit resumed after escalation keeps its slug and so
  its root. DROPPED: a unit that reaches a terminal state has its root reaped with its worktree; a
  root whose worktree a crash already removed is reclaimed by the next step's backstop, its unit not
  live. Concurrent: two units, and two speculation lanes of one unit, get distinct roots because
  their worktree names differ; the backstop spares every live unit's root by the liveness test it
  applies to the cache sibling. Crash-resume: a step that dies mid-gate leaves the root; a resumed
  unit reuses it, and a terminal unit's is reclaimed by the next step, its processes reaped first
  (`reap_then_remove_dir`). Cold start: derived from the worktree path alone. Existing data: a
  `cargo-mutants-<slug>` directory left by an earlier binary matches no arm and is never reclaimed
  (Notes).
- *Criterion 4.* Empty: a project with no marker gets the no-set workflow and line; a set whose
  `files` is empty writes no file; a set whose `detect` is empty never matches. Repeated: a rerun
  writes nothing and reports already initialized. Reverted: a set file the project deleted is
  written again on the next run. DROPPED: a file later removed from a set's `files` is no longer
  written and copies already in projects are left alone; a project whose marker was removed matches
  no set and keeps its workflow; a listed file removed from this repository fails the build naming
  it. Concurrent: two inits racing in one project behave as today's `write_if_absent`; out of scope.
  Crash-resume: each file is written on its own, so a rerun completes what an interrupted run left.
  Cold start: pure over the project root's files and the embedded sets. Existing data: a project
  with a workflow keeps it, gets no gate added and no no-set line, and gets the matched set's absent
  files; this repository already holds every Rust-set file, so `rigger setup` here writes none.
- *Criterion 5.* Empty: a unit branch with no commit since the run branch passes, and a repository
  with no merge base passes with the script's message. Repeated, reverted, DROPPED, concurrent: a
  function of the branch's history only; a branch rebuilt with its test commit first passes.
  Crash-resume: out of scope - one script run. Cold start: reads git only. Existing data: a project
  scaffolded by an earlier binary keeps its placeholder gate, since the workflow is never rewritten
  (Notes).
- *Criterion 6.* Empty: a file with no banned token reports nothing. Repeated: a line holding a
  token twice reports once. Reverted: a later change that brings a token back fails the rule.
  DROPPED: with the cache-home root deleted, `rigger result` reclaims only the spawn's agent scratch
  and `rigger validate` measures only `agent-scratch` as registered scratch. Concurrent,
  crash-resume: out of scope - a pure scan. Cold start: pure. Existing data: an empty
  `<cache home>/rigger-mutants` directory on an operator machine is never read or removed again
  (Notes).
- *Criterion 7.* Out of scope for every corner - it runs the lanes over the integrated result.
- *Global constraints, every criterion.* Hyphens and ASCII: the `scaffold/` files are added lines
  too. No new event type: no criterion emits or folds one. No new crate dependency: the build script
  reads `files` with the standard library, and `serde_yaml` is already a dependency of the root
  package. Both lanes: the resolver, the template sets and rule 4 compile and run in both. Operator
  binary: no unit installs or replaces it, and no test needs it. This repository's pinned
  definition: untouched by every unit, as UNIT ORDER AND BASE decides.

**OUT OF SCOPE.** A language flag for `rigger init` (one set exists, and detection picks it);
per-language persona seeds (`rust-engineer` stays the scaffold's implement and check-in agent); the
core's cargo build environment (`CARGO_TARGET_DIR`, `BuildEnv`), which is not a gate; `boundary` and
`audit` templates; the built-in instruction prose on mutation testing.

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
pub struct Requirement { pub name: String, pub at: Option<std::path::PathBuf> }
pub struct GateRequirements { pub gate: String, pub requires: Vec<Requirement> }
pub fn resolve_requirements(
    gates: &BTreeMap<String, rigger_domain::config::Gate>,
    path_var: &std::ffi::OsStr,
) -> Vec<GateRequirements>;
pub fn resolve_requirements_on_path(
    gates: &BTreeMap<String, rigger_domain::config::Gate>,
) -> Vec<GateRequirements>;
#[error("gate {gate:?} requires {requirement:?}, which is not an executable on PATH (config key: gates.{gate}.requires)")]
pub struct RequirementUnavailable { pub gate: String, pub requirement: String }

// crates/rigger-domain/src/spawn.rs
pub const UNIT_GATE_SCRATCH_PREFIX: &str = "rigger-gate-";
pub fn unit_scratch_slug(name: &str) -> Option<&str>;

// src/cli/setup.rs
struct TemplateSet { key: &'static str, set: &'static str, files: &'static [(&'static str, &'static str)] }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetSpec { detect: Vec<String>, gates: String, implement: Vec<String>, checkin: Vec<String> }
fn scaffold_workflow(set: Option<&SetSpec>) -> String;
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

Scaffold text: the head comment's `Replace the gate commands with your own.` becomes `Its gates come
from the gate template set matching this project's language; with none, it declares no gates.`; the
check-in comment loses its `then sweeps mutants` clause; the check-in coverage becomes
`the whole spec diff passes every gate on the merged tree`; the implement stage's trailing
`# red -> green enforced around the change` is dropped, since a project with no set lists no gate.

`scaffold/rust/set.yml`, byte for byte between the fence lines:

```yaml
# The Rust gate template set: rigger init applies it when Cargo.toml is at the project root.
detect: [Cargo.toml]
gates: |
  build: { run: "cargo build --workspace", kind: core }
  test: { run: "cargo test --workspace", kind: core }
  lint: { run: "cargo clippy --workspace --all-targets -- -D warnings", kind: elevated }
  # The red-before-green gate: TDD made mechanical. A unit's first source commit must be
  # preceded by, or carry, a test change (.rigger/gates/red-before-green.sh).
  red-before-green: { run: "sh .rigger/gates/red-before-green.sh", kind: core }
  # The check-in mutation sweep ships unwired as .rigger/gates/mutation.sh (its header explains
  # each clause). To run it, declare it and list it in the checkin stage's gates:
  #   mutation: { run: "sh .rigger/gates/mutation.sh", kind: core, requires: [cargo-mutants] }
implement: [build, test, lint, red-before-green]
checkin: [build, test, lint]
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
| `ci_and_the_lanes_gate_run_one_script_that_derives_its_members` | `tests/ci_lanes.rs` | deleted; its CI half is `ci_runs_the_no_default_and_core_lanes_through_the_lanes_script` |
| `this_repository_wires_every_principle_gate_on_the_stages_it_guards` | `tests/principle_gates_wiring.rs` | deleted, with the repository mode of `missing_principle_gates` and `PrincipleGate::repo_command`; the module doc names the scaffold only |
| `the_fmt_clippy_and_build_gates_cover_the_workspace` | `tests/principle_gates_wiring.rs` | deleted; the Rust set's `--workspace` commands are pinned by criterion 4's scaffold test |
| `the_test_gate_covers_the_workspace_with_the_container_runtime` | `tests/principle_gates_wiring.rs` | deleted |
| `the_test_gate_runs_the_tests_in_a_worktree_that_predates_the_snippet`, `the_test_gate_fails_when_its_snippet_fails_to_source`, helper `run_test_gate` | `tests/principle_gates_wiring.rs` | deleted: they run this repository's inline `test` command |
| `the_audit_gate_passes_a_fresh_committed_catalog_silently`, `the_audit_gate_names_a_regenerated_uncommitted_catalog_without_failing_on_drift_alone`, `the_audit_gate_fails_red_assertions_with_its_own_diagnostic`, helper `run_audit_gate` | `tests/principle_gates_wiring.rs` | deleted: they run this repository's inline `audit` command |
| helper `repo_gate_command` | `tests/principle_gates_wiring.rs` | deleted |
| `the_mutation_gate_runs_the_shipped_script_and_init_writes_the_same_script` | `tests/principle_gates_wiring.rs` | first assertion deleted (criterion 2); the rest replaced (criterion 4) |
| `project_events_reads_this_projects_own_real_workflow_yml` | `crates/rigger-grounder/src/grounder/workflowdef.rs` | its gate assertion deleted; the stage and agent assertions stay |

OTHER TEST DISPOSITIONS:

| Test (current name) | File | Disposition | Criterion |
|---|---|---|---|
| `validate_fails_before_any_output_when_the_mutation_gate_has_no_cargo_mutants`, `validate_fails_at_run_start_when_the_scaffolded_mutation_gate_has_no_cargo_mutants_on_path` | `tests/cli.rs` | re-homed as `validate_refuses_before_any_output_when_a_gate_requirement_is_not_on_path` on a fixture gate `sweep` | 1 |
| `validate_reports_mutation_gate_declared_when_cargo_mutants_is_resolvable` | `tests/cli.rs` | re-homed as `validate_reports_each_gate_requirement_resolved_on_path` | 1 |
| `validate_reports_mutation_gate_declared_by_default_on_a_fresh_scaffold`, helper `path_with_no_cargo_mutants` | `tests/cli.rs` | deleted; `path_with_no_known_wrapper` stops staging a fake `cargo-mutants` | 1 |
| `validate_never_probes_for_cargo_mutants_when_no_mutation_gate_is_declared`, `validate_accepts_a_declared_mutation_gate_when_cargo_mutants_is_on_the_real_path` | `crates/rigger-config-files/src/config_store.rs` | replaced by `validate_refuses_the_first_missing_gate_requirement` and `validate_accepts_a_gate_named_mutation_that_requires_nothing` on a synthetic workflow | 1 |
| `mutation_gate_binary_available_finds_the_binary_on_path`, `mutation_gate_binary_available_errors_naming_the_binary_and_gate_id_when_absent`, `mutation_gate_binary_available_ignores_a_same_named_non_executable_file_on_path`, `mutation_gate_binary_on_path_reads_the_real_ambient_path` | `crates/rigger-gates-shell/src/gate.rs` | replaced by `resolve_requirements` tests on a synthetic `PATH`: found, missing, a same-named non-executable file, an empty name and a name holding `/` | 1 |
| `build_environment_report_reports_mutation_gate_declared`, `build_environment_report_reports_mutation_gate_not_configured` | `src/cli/validate.rs` | replaced by `gate_requirement_lines` tests | 1 |
| `two_units_gate_environments_never_share_a_mutants_root` | `crates/rigger-conductor/src/conductor.rs` | renamed `two_units_gate_environments_never_share_a_gate_scratch_root` | 3 |
| `an_implement_stage_gate_round_creates_no_mutants_directory` | `crates/rigger-conductor/src/conductor.rs` | renamed `a_gate_round_never_creates_the_gate_scratch_root`, reading `$RIGGER_GATE_SCRATCH` | 3 |
| `the_post_merge_re_gate_runs_in_its_own_scratch_worktree_never_the_repo` | `crates/rigger-conductor/src/conductor.rs` | its mutants-root assertion reads the gate scratch root | 3 |
| `worktree_remove_also_reclaims_the_sibling_mutants_root`, `unit_mutants_sibling_maps_a_unit_worktree_to_its_mutants_root_and_ignores_the_rest` | `crates/rigger-worktree-git/src/worktree.rs` | renamed to `..._gate_scratch_root...` on `rigger-gate-<slug>` | 3 |
| `worktree_remove_reaps_a_process_rooted_in_its_sibling_mutants_root_before_reclaiming_it` | `tests/reap_before_removal_periphery.rs` | renamed `..._sibling_gate_scratch_root_...` | 3 |
| the `MUTANTS` environment and `cargo-mutants-checkin` directories | `tests/checkin_mutation_diff_base_periphery.rs` | `RIGGER_GATE_SCRATCH` and `rigger-gate-checkin` | 3 |
| `scaffold_parses_into_a_valid_config` | `src/cli/setup.rs` | both renderings load and validate; the Rust rendering's gates and lists asserted; no placeholder gate | 4 |
| (new) `tests/template_sets_build.rs` | `tests/` | refuses an absolute path, a `..` segment and a missing file, naming the set and the path | 4 |
| `a_scaffolded_consumer_project_carries_every_principle_gate_and_checklist_line` | `tests/principle_gates_wiring.rs` | runs in a fixture with a root `Cargo.toml`; `PRINCIPLE_GATES` keeps only `red-before-green`, and `PrincipleGate` drops the fields no remaining entry reads | 4 |
| `every_reviewer_prompt_forbids_cargo_mutants` | `crates/rigger-conductor/src/conductor.rs` | renamed `every_reviewer_prompt_forbids_a_mutation_sweep` | 6 |
| the persona pins named in THE CORE NAMES NO GATE | `src/cli/mod.rs` | moved to `tests/principle_gates_wiring.rs` | 6 |
| `footprint_report_folds_a_none_mutation_root_to_a_zero_contribution`; every `mutation_scratch_*` and `reclaim_unit_mutation_scratch_*` test in `replay.rs`; `tests/mutation_scratch_root_periphery.rs` and `tests/mutation_scratch_reap_base_guard_periphery.rs` whole; in `tests/spawn_scratch_reap_authorized_root_periphery.rs` the three tests whose names hold `mutation_scratch_dir`; in `tests/cli.rs` `populated_mutation_scratch`, `registered_mutation_scratch_root`, `assert_a_speculation_exit_reaps_every_lanes_mutation_scratch` and every test whose name holds `mutation_scratch` | several | deleted | 6 |
| `a_dotdot_spawn_id_never_escapes_the_registered_scratch_roots`, `a_leading_slash_spawn_id_never_collapses_the_reclaim_to_its_registered_root`, helper `assert_a_hostile_spawn_id_spares_its_neighbours`, `validate_reports_footprint_by_category_and_flags_a_dead_share_breach`, `validate_flags_registered_scratch_roots_dead_share_scoped_to_real_spawn_liveness_in_the_store`, `validate_flags_a_prior_abandoned_runs_orphan_even_when_a_later_run_reuses_the_identical_spawn_id` (`tests/cli.rs`); `footprint_report_measures_every_category_on_a_seeded_fixture_tree`, `footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn`, `footprint_advisories_name_reset_build_cache_for_every_class_it_reclaims` (`src/cli/mod.rs`); `spawn_scratch_path_and_mutation_scratch_path_hex_escape_a_dotdot_id_so_it_can_never_escape` (`replay.rs`) | several | the cache-home half dropped, every other assertion kept | 6 |
| `is_reapable_base_accepts_a_root_that_is_not_named_dot_rigger_tmp_at_all` | `crates/rigger-process/src/reap.rs` | its fixture root renamed from `rigger-mutants`; the assertion kept | 6 |

Deferrals and leftovers:
- Re-wiring the mutation gate into this repository's check-in stage (issue #32) is OUT. That change
  also gives this repository's `mutation` gate `requires: [cargo-mutants]` and corrects the comment
  above it, which still says declaring the id requires `cargo-mutants`; both are edits to the pinned
  definition, made by the operator outside a run.
- A `cargo-mutants-<slug>` directory under a scratch root and an empty `<cache home>/rigger-mutants`
  directory left by an earlier binary are the operator's to delete; nothing reclaims them.
- A consumer project scaffolded by an earlier binary keeps its placeholder gates; replacing them is
  the consumer's edit.

## Global constraints

- Hyphens, never em or en dashes, and ASCII only, in every added line.
- No new event type; no new crate dependency.
- Both feature lanes green (fmt, clippy -D warnings, test on default and --no-default-features).
- The operator's installed rigger binary is never replaced or modified by any unit.
- No unit edits this repository's `.rigger/workflow.yml`, `.rigger/agents/` or `.rigger/instructions/`.
- Fewest moving parts: one requirement resolver, one gate scratch root, one scratch-name predicate,
  one template-set embedding.
- A criterion whose test cannot be written as specified escalates naming the blocking fact; no unit
  narrows a fixture, re-introduces a gate name or keeps a live-workflow read to get green.
- Flagged for the adjudicator, which the gates cannot see: each row of LIVE-WORKFLOW TESTS is shown
  deleted or re-homed in the diff, and each comment criterion 6 rewords names the generic mechanism.

## Done when

- [ ] a test proves GATE REQUIREMENTS RESOLVE THROUGH ONE PATH: `rigger validate` prints `gate sweep: requires <name> at <path>` for a fixture gate whose `requires` names a stub executable first on `PATH`,
  and without that stub refuses before any output with the one requirement message naming the gate,
  the executable and `gates.sweep.requires`, while a fixture gate named `mutation` that declares
  nothing validates with no `cargo-mutants` on `PATH`, asserted in `tests/cli.rs`. This criterion
  OWNS the `requires` key, the resolver, the refusal, the per-gate validate lines and the removal of
  the mutation probe, `MUTATION_GATE_ID` and the mutation validate line; the core-wide token rule is
  criterion 6's, NOT this one's.
- [ ] a test proves CHECK-IN GATES RUN ONLY AT CHECK-IN, ON A FIXTURE: a `RecordingRunner` records no run of the gate only the fixture's check-in stage lists before the last run of the gate only its fan-out implement stage lists,
  and records as many check-in-only runs for a three-criterion spec as for a two-criterion one,
  asserted in `crates/rigger-conductor/src/conductor.rs` beside
  `a_stage_needing_the_fan_out_template_becomes_ready_once_every_criterion_unit_integrates`. This
  criterion OWNS that test and every disposition in the Notes table LIVE-WORKFLOW TESTS; the
  scaffold's own pins are criterion 4's, NOT this one's.
- [ ] a test proves THE GATE SCRATCH ROOT IS NAMED AND REAPED GENERICALLY: a gate run in a unit worktree is handed `RIGGER_GATE_SCRATCH` naming that worktree's `rigger-gate-<slug>` sibling, distinct per unit,
  and `reclaim_orphan_scratch` reclaims a `rigger-gate-<slug>` whose unit is not live while sparing a
  live unit's, asserted in `two_units_gate_environments_never_share_a_gate_scratch_root` and beside
  `reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas`. This
  criterion OWNS the variable, the prefix, the runner parameter, the scratch-name predicate at every
  scratch walk and the root line and gate-environment paragraph of `.rigger/gates/mutation.sh`; the
  removal of the spawn-keyed cache-home root is criterion 6's, NOT this one's.
- [ ] a test proves INIT SCAFFOLDS GATES FROM THE PROJECT'S TEMPLATE SET: `rigger init` beside a root `Cargo.toml` writes the Rust set's gates, stage gate lists and listed files into a fixture project,
  each listed file with the bytes of this repository's copy, while a project matching no set gets a
  workflow declaring no gates and the no-set line, and no rendering wires a placeholder gate,
  asserted in `tests/principle_gates_wiring.rs` and `src/cli/setup.rs`. This criterion OWNS
  `scaffold/`, the build-script embedding, the detection, the rendering, the no-set line and the
  first paragraph of `.rigger/gates/mutation.sh`; the red-before-green verdict is criterion 5's, NOT
  this one's.
- [ ] a test proves THE SCAFFOLDED RED-BEFORE-GREEN GATE FAILS A TEST-LESS COMMIT: in a Rust fixture repository after `rigger init`, the scaffolded `red-before-green` command run under `sh -c` fails a test-less `src/` commit,
  naming that commit, and passes a branch whose test commit comes first, asserted in
  `tests/principle_gates_wiring.rs`. This criterion OWNS only that test; the set that wires the gate
  is criterion 4's, NOT this one's.
- [ ] a test proves THE CORE NAMES NO GATE: rule 4 of `tests/boundary_audit.rs` finds no line under `src/` or `crates/` holding a banned gate or tool token and reports a fixture file that holds one by file and line.
  This criterion OWNS the rule and the removal of every occurrence criteria 1, 3 and 4 leave,
  including the spawn-keyed cache-home root; each of those criteria's own removals is theirs, NOT
  this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
