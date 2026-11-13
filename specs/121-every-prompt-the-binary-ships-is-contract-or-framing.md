# 121 - Every prompt the binary ships is contract or framing

**Goal:** every prompt text and tool description the binary puts in front of an agent as
instruction is CONTRACT (a shape the binary parses,
a tool name or argument) or FRAMING (a fact of the task only the binary knows); a PRESCRIPTION
(method, cadence, discipline, review or test-authoring procedure, planning policy, a language or
toolchain word) lives in a surface the consumer owns: the overridable built-in layer, the
personas (this repository's `.rigger/agents/*.md` and the scaffold seeds), the skills, or a declared
workflow key. These prescriptions stand in the binary, each with its homes. The graph-first lookup
method has three homes: `write_lookup_pointer` (`crates/rigger-conductor/src/conductor.rs:14209`, in
every grounded task prompt, a mandatory `grep-fallback:` record included), the PreToolUse grep-guard
(`crates/rigger-driver/src/hooks.rs:30-32`, merged into every headless spawn's `--settings` by
`spawn_config_args`, `crates/rigger-driver/src/driver/mod.rs:390`, and into the operator's
`.claude/settings.json` by `install_lookup_hook`, `src/cli/setup.rs:1329`) and the `lookup` helper.
The helpers have two homes on the headless hosts and the binary's wins: `HELPER_AGENTS`
(`crates/rigger-driver/src/hooks.rs:51-60`) is passed inline, so an edited `.claude/agents/lookup.md` never reaches a headless
spawn. The communication cadence has five homes outside the `communication` built-in: `EMIT_PROTOCOL`
"the moment you make it" (`conductor.rs:13191`), `review_protocol` "the moment you raise it"
(:13229), `DECISIONS_HEADER` "do not contradict them; supersede explicitly if you must" (:13419),
`PLAN_PROTOCOL`'s "Propose each refinement the moment you decide it" (:13202) and the shim's
"Re-check the rigger_peers tool between your own actions" (`shim/shim.mjs:150`). The progress cadence
has four homes in four wordings: `progress_framed` (`crates/rigger-driver/src/driver/mod.rs:63`, the
framing both process hosts append to every task, each with its own report verb),
`workflows/rigger.js:676-678` (every workflow-path worker), `shim/shim.mjs:254-257` (every shim
worker) and the `rigger_progress` description (`crates/rigger-dash/src/mcpserver.rs:785`).
`DOC_ONLY_ROUND` (`conductor.rs:14745`) carries a test-authoring rule whose
doc says it "lives here alone, never copied into a persona". `MAX_UNIT_ADDED_LINES` (:13180) is a
planning policy as a constant every project gets. `PLAN_PROTOCOL` carries two planning rules the
code does not enforce (an unmatched unit still runs). The REQUIRED PATTERN audit (`conductor.rs:1420`,
:13325-13328) is a procedure asked and checked by prompts only. `ROLE_SDET_AUTHOR`
(`crates/rigger-domain/src/spawn.rs:113`)
adds a spawn by fixed agent id that no workflow key declares (`spawn_sdet_author`,
`conductor.rs:5517`, :5542). `workflows/rigger.js` names `cargo` (:650, :685, :919), a mutation tool
(:685) and one operator machine's disk layout (:699), and hard-codes a cache path (:919);
`rigger_scratch` (`crates/rigger-dash/src/mcpserver.rs:786`) names `CARGO_TARGET_DIR`. The `planning-a-spec` skill
(`crates/rigger-domain/src/docs.rs:440`) carries this repository's house constraints and Rust path
examples, and every shipped skill is rewritten on drift, so no consumer can replace one. The scaffold
names `agent: rust-engineer` (`src/cli/setup.rs:1802`, :1817) and seeds the Rust persona whichever
gate set matched. And nothing stops a new prescription entering a prompt: no test enumerates the
prompt surface. This spec moves each prescription to its owner's surface and lands a boundary gate
that fails on any string literal of 40 or more characters with a space in a prompt-bearing file that
is neither registered as contract or framing nor allow-listed as an operator message. At 8e06d136 the
prompt-bearing files of the production text (outside `tests/`, cut at each file's test module) of
`crates/` and `src/`, the files spelling `SpawnOpts`, `SpawnHost` or `prompt_surface` on a line that
is not a `//` comment, are 10 holding 139 such literals (`conductor.rs` 100, the headless host driver
module of `rigger-driver` 16, `src/cli/critique.rs` 9, `crates/rigger-conductor/src/canary_store.rs` 5,
`crates/rigger-domain/src/agent.rs` 3, `crates/rigger-driver/src/driver/mod.rs` 3, `crates/rigger-driver/src/driver/cli.rs` 2, `crates/rigger-driver/src/driver/workflow.rs` 1,
`crates/rigger-conductor/src/held.rs` 0, `crates/rigger-driver/src/driver/replay.rs` 0; `crates/rigger-gates-shell/src/gate.rs`
names `SpawnOpts` only in `///` doc lines and stays outside), a set that grows by every renderer file
criterion 16 moves onto `prompt_surface`: after it, 22 files holding 302 such literals at 8e06d136's
count (the 10 above plus `src/cli/mod.rs` 95, `crates/rigger-dash/src/mcpserver.rs` 40, `src/cli/guard.rs` 15,
`crates/rigger-domain/src/review.rs` 8, `src/cli/prime.rs` 2, the prime renderer's two literals
criterion 16 moves out of `src/cli/setup.rs`, and `crates/rigger-driver/src/hooks.rs` 2, which spells `SpawnHost` once the
resolver lives there, `crates/rigger-domain/src/instructions.rs` 1, which spells `prompt_surface` once
`compose` reads its heading from there; `src/cli/run.rs` and `src/cli/eval.rs` call the resolver and pass its value
without naming the type, so they stay outside; and the five lib files that declare `pub mod
prompt_surface;`, `src/lib.rs` and the `lib.rs` of `rigger-domain`, `rigger-conductor`, `rigger-driver`
and `rigger-dash`, 0 each), and each crate's new `prompt_surface` module, whose literals are the
registry itself; the figure is the set at 8e06d136, before any module exists. Tool results, errors
and CLI output an agent reads back are data, not instruction, and are outside this claim (THE SOURCE
SWEEP's tool-output category); a hook's denial or exit-2 text is registry text, not tool output. Of
`src/cli/guard.rs`'s 15, 7 are registered (all FRAMING: the deny reason, the five argument errors, the
unreadable payload) and 8 are operator messages, so the 302 stands.

## Design

**BASE, decided here.** Spec 121 lands on `rigger-run`. Its citations are read at 8e06d136; the
symbol governs and the line is a pointer. At that base: `RIGGER_COMMUNICATION` and the free function
`build_system_prompt` are gone (`RunCtx::build_system_prompt`, `conductor.rs:11368`, stays) and the
`communication` built-in (`crates/rigger-domain/src/instructions/communication.md`) carries the
discipline; `REVIEWER_DISCIPLINE` (`conductor.rs:13252`) holds three sentences and is `pub`;
`gate_evidence_block` (:591) holds only its rows and framing sentence; no critique method constant
remains, and the method stands in the adversary's and the adjudicator's `## When you critique a spec or
a plan` sections, persona (`.rigger/agents/adversary.md:17`, `.rigger/agents/adjudicator.md:32`) and
scaffold template (`src/cli/setup.rs:1987`, :2054); each of the four sections holds the numberless size
bullet "- Unit size: a unit over the cap your prompt states is a BLOCKING finding." (`adversary.md:34`,
`adjudicator.md:43`, `setup.rs:2020`, :2079), and the adjudicator's judging rule also names "a unit over
the cap the prompt states" (`adjudicator.md:46`, `setup.rs:2086`); the DAG critique prompt renders
`unit_size_cap()`'s text verbatim as its cap framing, `unit_size_cap` (`conductor.rs:13184`) building it
from `MAX_UNIT_ADDED_LINES` (:13180). The MARKERS rule is `language_marker_hits`
(`tests/common/fixtures/language_markers.rs:9`), declared by `mod language_markers;` and `pub use
language_markers::*;` in `tests/common/fixtures/mod.rs`, the file `src/lib.rs` and `src/main.rs` mount
as `#[cfg(test)] #[path = "../tests/common/fixtures/mod.rs"] mod test_support;`, so
`src/cli/setup.rs`'s test module reaches it as `crate::test_support::language_marker_hits`. That test
module holds the shipped-text helper `every_shipped_agent_text()` (`setup.rs:3988`) and the
production-line cut of the pin
`no_built_in_entry_persona_helper_or_production_line_names_mutation_testing` (`opens_test_module`,
:4801, and its `lines[..end]` cut, :4817). The `working-discipline` body's `lookup` bullet
(`crates/rigger-domain/src/instructions/working-discipline.md:120-121`) carries, verbatim, "The same
rule binds you: the graph first, a literal `grep` only when it is strictly necessary, and each such use
recorded." (criterion 1 cuts it). The open-exceptions list in `docs/handbook/authoring-loops.md`'s `##
What rigger prescribes` section names exactly the prescriptions EXCEPTIONS LIST assigns to this spec's
criteria, its shared item reading "the toolchain words in `workflows/rigger.js` and the
`rigger_scratch` description". The HIT ENUMERATION below is complete at that base: no other
phrase-list or MARKERS hit stands in a registry surface or a script prompt.

**WHAT MOVES AND WHAT STAYS, decided here.** Each moved sentence lands verbatim in its new home in
the same commit that cuts the binary copy. A sentence carrying a slot lands with the slot replaced by
the noun phrase this Design names. Where one prescription has several homes in different words, the
wording named here lands and the other homes' wordings are cut as its duplicates. The binary keeps
every CONTRACT and FRAMING part: tool names and arguments, event shapes, the verdict and finding
grammar, the `required` list, the self-report, heartbeat and scratch protocols, the unit, its
criterion, paths, gate evidence, the REQUIRED list, the delta and the grounding sections (their
headers less the method clauses criterion 4 cuts). A task imperative stays FRAMING when it does or
continues the work the binary assigns, its scope included ("Finish and report; do not start over.",
"Fix exactly these - do not start over:", "Resolve the conflicts in these paths only, preserving
both sides' intent", "Continue where you left off.", the DAG critique opener's "judge it against the decomposition rules in
your persona", which names the scope those rules apply to), or states a limit the harness enforces
(`REVIEWER_DISCIPLINE`); an imperative that says how to read, weigh or answer the material a prompt
shows is a prescription, cut as a duplicate or moved under these rules.

EXCEPTIONS LIST, decided here: `authoring-loops.md`'s `## What rigger prescribes` holds, under its
**Open exceptions.** lead-in sentence, a list of the prescriptions this spec owns, the rule's open
exceptions. Each item is struck by the criterion that lands it, in the same commit: the lookup
pointer's method sentences by 1; the cadence clauses in `EMIT_PROTOCOL` and `review_protocol` by 4;
the progress cadence in its four host homes by 5; `DOC_ONLY_ROUND`'s rule sentence by 6;
`MAX_UNIT_ADDED_LINES` and `unit_size_cap`'s second sentence by 7; `PLAN_PROTOCOL`'s guidance by 8;
the REQUIRED PATTERN audit sentences by 9; the shared item "the toolchain words in
`workflows/rigger.js` and the `rigger_scratch` description" by 11 and 12 together: a list item shared
by two criteria is NARROWED by the first of them to land, to the remaining concern ("the toolchain
words in the `rigger_scratch` description" when 11 lands first, "the toolchain words in
`workflows/rigger.js`" when 12 does), and STRUCK by the second. Criterion 17, which lands after every
striking criterion (1 to 15, then 16 and 18), removes the emptied list with its lead-in sentence; each
striking criterion keeps only its own strike. The method clauses criterion 4 cuts from the grounding
headers and the run notes are not on the list, so no strike covers them.

HIT ENUMERATION, decided here: every phrase-list hit (criterion 17's list) and every MARKERS hit
(`language_marker_hits`) in a registry surface or a script prompt at 8e06d136, with its disposition, by one
rule: a hit inside a prescription moves with that prescription to its named home (or is cut as the
duplicate of a home that already states it), never reworded (a clause whose pronoun names its
header's subject lands with that subject in the pronoun's place, as a slot's fill does); a hit inside a
framing or contract sentence is reworded with the replacement named here, since the sentence prescribes nothing.

| Site | Hit | Disposition |
| --- | --- | --- |
| `src/cli/mod.rs:5526` `GREP_GUARD_MESSAGE` | "for code lookups" | prescription, cut as the `working-discipline` sentence's duplicate (criterion 2) |
| `conductor.rs:1420` `PriorFailure::block` PATTERN | "audit the whole tree" | prescription, cut as the duplicate of `working-discipline`'s bounded-audit bullet (criterion 9) |
| `conductor.rs:13326` `ReviewRound::block` PATTERN | "audit the whole tree" | prescription, cut as the same duplicate (criterion 9) |
| `conductor.rs:13191` `EMIT_PROTOCOL` | "the moment" | prescription, cut as the `communication` built-in's duplicate (criterion 4) |
| `conductor.rs:13229` `review_protocol` | "the moment" | prescription, cut as duplicate (criterion 4) |
| `conductor.rs:13420` `DECISIONS_HEADER` | "do not contradict", "supersede explicitly" | prescription, cut as duplicate (criterion 4) |
| `conductor.rs:13232` `review_protocol` | "address or refute" | prescription, cut as the duplicate of the paragraph criterion 4 lands in `working-discipline` (criterion 4) |
| `conductor.rs:13455` `CODE_NEIGHBORHOOD_HEADER` | "read first" | prescription, moves to `working-discipline` (criterion 4) |
| `conductor.rs:13504` `DESIGN_INTENT_HEADER` | "honor them" | prescription, moves to `working-discipline` (criterion 4) |
| `conductor.rs:14133` `write_capped_lessons` | "do not repeat" | prescription, moves to `working-discipline` (criterion 4) |
| `conductor.rs:14158` `write_capped_findings` | "address or refute" | prescription, moves to `working-discipline` (criterion 4) |
| `conductor.rs:13202` `PLAN_PROTOCOL` | "the moment", "do not write code", "scope creep" | prescriptions, move to the planner persona and seed (criterion 8) |
| `conductor.rs:13202` `PLAN_PROTOCOL` | MARKERS `matches` (twice) | framing sentences, reworded "pairs your unit with its baseline" and "is none of the ids below" (criterion 8) |
| `conductor.rs:14211` `write_lookup_pointer` | "not by grepping" | prescription, moves to `working-discipline` (criterion 1) |
| `conductor.rs:14745` `DOC_ONLY_ROUND` | "never a new test" | prescription, moves to the sdet-author persona (criterion 6) |
| `crates/rigger-driver/src/driver/mod.rs:65` `progress_framed` | "keep it flowing" | prescription, cut as duplicate of the moved progress bullet (criterion 5) |
| `crates/rigger-dash/src/mcpserver.rs:785` `rigger_progress` | "never batch" | prescription, cut as duplicate (criterion 5) |
| `crates/rigger-dash/src/mcpserver.rs:786` `rigger_scratch` | "the moment"; MARKERS `cargo` (`CARGO_TARGET_DIR`) | contract sentences, reworded "Reaped once your result records." and "build cache" (criterion 12) |
| `rigger.js:676-678` `progressNote` | "do not batch", "keep it flowing" | prescription, moves to the `communication` built-in (criterion 5) |
| `rigger.js:650` `workdir` | MARKERS `cargo` | framing, reworded "your build tool's runs" (criterion 11) |
| `rigger.js:685-686` BUILD LOCATION | MARKERS `cargo`, `clippy` (and "mutants"), `CARGO_TARGET_DIR` | framing, reworded "every command your build tool runs ... (anything that builds or tests)" and `${req.build_cache_env}` (criterion 11) |
| `rigger.js:687` BUILD LOCATION | `target/` (the toolchain's default build directory) | framing, reworded "A build cache inside the worktree itself is a defect that fills the disk - never create one." (criterion 11) |
| `rigger.js:699` SCRATCH POLICY | "the moment"; MARKERS `cargo` (twice) | the disk-layout parenthetical is a prescription, dropped (the reaper is its contract); the reaper sentence is contract, reworded "It is reaped once your result records"; the export sentence is framing, reworded with `${req.build_cache_env}` (criterion 11) |
| `rigger.js:918-920` step courier | MARKERS `cargo`, `CARGO_TARGET_DIR`; "feature lane" | framing; the prefix and its sentence dropped (the binary sets the variable on every branch), "running their gates inline" (criterion 11) |
| `rigger.js:55` phase detail | MARKERS `cargo` | not a prompt (the `/workflows` phase label): an operator region (criterion 18), stays as written |
| `shim.mjs:149-150` `renderPeers` | "do not contradict", "supersede explicitly", "between your own actions" | prescription, cut as duplicate (criterion 4) |
| `shim.mjs:254-257` `progressNote` | "do not batch", "keep it flowing" | prescription, cut as duplicate (criterion 5) |

No other registry surface carries a hit at the base: not `gate_evidence_block`, `REVIEWER_DISCIPLINE`,
the DAG critique prompt or the critique framing (`crates/rigger-domain/src/review.rs:325`), not the
canary's texts, `guard.rs`'s deny reason or the other MCP descriptions, and not a text the class table
adds beside the hit rows (`UPHELD_PROTOCOL`, the run-notes block, `RECORD_RECOVERY`, the refused-record
note, the resume prompt, `CRITIQUE_RECORDED_FINDINGS`).

**THE LOOKUP METHOD, decided here (criterion 1).** `write_lookup_pointer` becomes one framing
paragraph: "The knowledge graph answers lookups through three verbs:" followed by the three verbs
with their one-line jobs as at the base. Its two guidance sentences move verbatim into the
`working-discipline` built-in's `## Fan out the mechanical work` section, in place of the `lookup`
bullet's last sentence "The same rule binds you: the graph first, a literal `grep` only when it is
strictly necessary, and each such use recorded." (criterion 1 cuts it, one rule keeping one home, and
owns its pins):
"Look things up in the knowledge graph, not by grepping the project's sources." and "If the graph
cannot answer and you fall back to grep over the project's sources, record it with `rigger progress
<id> 'grep-fallback: <what the graph did not answer>'` - one line before moving on - so the gap is
visible in the log; filtering your own build or gate output is not a fallback." with its Bash-form
slot "`rigger progress <id> 'grep-fallback: <what the graph did not answer>'`" replaced by "a progress
line `grep-fallback: <what the graph did not answer>` through the progress verb your task or your tool
list names", so it agrees with criterion 5's progress bullet. The `grep-fallback:` prefix stays a
CONTRACT shape (`progress::GREP_FALLBACK_PREFIX`, `crates/rigger-domain/src/progress.rs:91`, which
`metrics.rs:1150` and the dash count); nothing in the binary demands it. The existing pin
(`conductor.rs:21228-21232`, "the pointer must carry the grep-fallback reporting instruction") moves
to the built-in body. The `using-rigger` skill's sentence (`docs.rs:184`) stays: a skill is the
consumer's surface (criterion 14).

**THE GREP GUARD IS DECLARED, decided here (criterion 2).** `Defaults` (`crates/rigger-domain/src/config.rs:748`) gains
`grep_guard: bool`, default false. Declared true: `rigger setup`'s `install_lookup_hook` installs the
PreToolUse entry as at the base, and every headless spawn's `--settings` carries it. Absent or false:
`install_lookup_hook` removes rigger's own entry from `.claude/settings.json`: every inner hook
under `PreToolUse` whose command equals `GREP_GUARD_COMMAND`, whatever its matcher, a block dropped
only when its inner array becomes empty, an emptied `PreToolUse` array and an emptied `hooks` object
removed in turn, so a settings file rigger alone populated returns to its pre-rigger shape (the
byte-equal oracle: its input is the settings file before any guard was installed, the foreign hooks
alone; after the guard is installed into it and then removed, the file equals that input as
reserialized by the merge `install_into` (`src/cli/setup.rs:1353`) applies), every other hook kept, writing only when the merge changes
the file. Install keys on the same rule: the guard is present when any inner `PreToolUse` hook has
that command, and an install with the key true replaces every inner hook carrying
`GREP_GUARD_COMMAND` whose matcher differs from `GREP_GUARD_MATCHER` (`crates/rigger-driver/src/hooks.rs:30`), so exactly one
entry with the current matcher remains (`cmd_setup` parses `.rigger/workflow.yml` after `init_project` to read the
key and fails loudly on a parse error, as `get_referenced_agent_ids`, `setup.rs:190`, already does); and `hooks::install_session_settings` takes the flag and merges only the
prime hook and the status line. Corners: with the key off the removal reads `settings.json` only if it exists and
creates no `.claude/` directory (it never reaches the settings installer's directory creation,
`setup.rs:1346`), so an absent `settings.json` with the key off writes nothing and
reports `AlreadyCurrent` (a corner of `install_lookup_hook` alone: through `rigger setup`, `init_project`
has already written `settings.json`, `setup.rs:228-236`); a removal that changes the file reports
`Refreshed`, which `cmd_setup`, holding the key it read, prints with the key off as "removed the
graph-first lookup hook: `defaults.grep_guard` is not declared" (its two installed lines,
`setup.rs:836-849`, print only with the key true; criterion 2 owns the line and its pin);
`hooks::carries_session_settings` takes the same flag, which its caller (`src/cli/validate.rs:869`)
reads from the loaded config; `session_hooks_advisory` (`src/cli/validate.rs:867-879`) names the
grep-guard among the missing hooks only when `defaults.grep_guard` is declared, and names a present
`GREP_GUARD_COMMAND` hook as stale when the key is false, telling the operator to run `rigger
setup`; criterion 2 owns both. A spawn's flag comes
from SPAWN HOST RESOLUTION, never set by hand;
`spawn_config_args` takes `host: &SpawnHost` and passes `host.grep_guard` to
`install_session_settings`, both `build_args` callers passing `&opts.host`. `cmd_grep_guard` reads no key: a `settings.json` written while
the key was true keeps denying after the key flips false until the next `rigger setup`, the operator's
standing step after any workflow key change; accepted. The hook mechanism and `rigger grep-guard` stay.
`GREP_GUARD_MESSAGE` (`src/cli/mod.rs:5526`) keeps only the guard's rule and its escape hatch: its
method clause "use rigger_ground / rigger_graph for code lookups; " is a prescription, CUT, never
reworded, since criterion 1's `working-discipline` sentence is that method's one home, so the binary
says it nowhere. The message then reads exactly "grep is for literal
text - end the command with a `# --literal` comment to proceed", CONTRACT (the guard parses the
marker). Criterion 2 owns the cut and its pin (the message equals that text), the edited assertion
at `tests/cli.rs:35001`, and the re-rendered `skills/using-rigger/SKILL.md` and
`docs/handbook/using-rigger.md`, which interpolate the message (`docs.rs:199`) and carry the
using-rigger sentence (`docs.rs:190-198`) criterion 2 conditions: "and installs a PreToolUse hook"
becomes "and, when the workflow declares `defaults.grep_guard: true`, installs a PreToolUse hook", and
"The graph stays the path of least resistance in this very session" becomes "With the guard
installed, the graph stays the path of least resistance in this very session". `SCAFFOLD_WORKFLOW` declares `grep_guard: true` under `defaults`, so a
freshly scaffolded project behaves as at the base (SCAFFOLD DEFAULTS).

**SPAWN HOST RESOLUTION, decided here (criteria 2 and 3).** The host values a headless spawn
needs, the guard flag and the helper texts, are resolved by one function,
`rigger_driver::hooks::resolve_spawn_host`, into one value, `agent::SpawnHost`, a struct beside
`SpawnOpts` (`crates/rigger-domain/src/agent.rs:83`) of `grep_guard: bool` (from
`defaults.grep_guard`) and `helpers: Vec<HelperCopy>` (`rigger_domain::agent::HelperCopy`, defined beside `SpawnHost`;
`rigger_driver::hooks::resolved_helpers` returns it). The resolver lives in
`rigger-driver` beside `hooks::resolved_helpers` because the domain crate reaches neither the loaded
`Config` (`rigger-config-files`) nor the filesystem; it takes `&Defaults`
(`config.rs:748`), which every caller holds as `cfg.workflow.defaults`.
Criterion 2 lands `resolve_spawn_host(defaults: &Defaults) -> SpawnHost` with the `grep_guard` field
only, so no unit lands a parameter nothing reads; criterion 3 adds the `repo_root: &Path` parameter,
consumed inside the resolver (the root is no field), and the `helpers` field it fills once through
`hooks::resolved_helpers(repo_root)`, the same value `definition_hash` folds, the return becoming
`Result<SpawnHost, Error>`: a copy that cannot be read or parsed fails the resolver, the
infrastructure fault THE OPERATOR'S HELPER COPY decides. `SpawnOpts` gains one field, `host:
SpawnHost`, and every construction sets it only from a resolved value, never a field by hand: the
five conductor sites (`conductor.rs:4823`, :6159, :6843, :8517, :11329) through one `RunCtx` accessor
that only reads a named `Deps` field, `Deps::spawn_host`, and never resolves. `rigger step`
(`cmd_step`, `src/cli/run.rs:447`) calls `resolve_spawn_host` ONCE, before `enforce_definition_pin`
(`src/cli/run.rs:166`), and hands that value to the conductor in `Deps::spawn_host`, so every spawn
the step parks or runs carries that one value; `rigger critique`'s (`src/cli/critique.rs:320`) and
the canary's (`canary_opts`, `canary_store.rs:573-579`) values are resolved once at their start in
`src/cli` the same way (`cmd_critique`; `src/cli/eval.rs:228`, which hands the value to `run_canary`
as a parameter). The binary builds `Deps` at four sites, each the resolution point of its own
command and none another's: `cmd_step` (`src/cli/run.rs:867`, `rigger step`), `run_cli`
(`src/cli/run.rs:1781`, `rigger run` on the cli driver), `run_workflow` (`src/cli/run.rs:2247`,
`rigger serve` and `rigger run --driver workflow`) and `cmd_replay` (`src/cli/mod.rs:2861`, the
replay of a candidate config in its isolated store). Each calls `resolve_spawn_host` once before
`conductor::run` and fills `Deps::spawn_host` with that value; `run_cli` and `run_workflow` pass the
same value to `fresh_run_if_requested`, their one `definition_hash` read. `cmd_replay` has one root: it resolves its `SpawnHost` inside `materialize_config_at_rev` at the
`--against` checkout root, from the candidate config's defaults, before that worktree is removed, and
that one value feeds both `definition_hash` and `Deps::spawn_host`, so the replay's definition and its
spawns carry the same helper copies (the checkout's tracked `.claude/agents/` when present, the
embedded text otherwise). With the critic and the canary, which resolve at their start,
there are six resolution points. The production `SpawnOpts { .. }` sites and what fills their `host`:
the five conductor sites (`reviewer_spawn_opts` :4823, `run_single_stage` :6159, `run_speculation`
:6843, `re_plan` :8517, `spawn_conflict_resolution_implementer` :11329, all `conductor.rs`) through the accessor
from `Deps::spawn_host`; `canary_opts` (`canary_store.rs:578-579`) from the canary's value; `critique`
(`src/cli/critique.rs:320`) from the critic's. These are every `SpawnOpts { .. }` construction in
production code. Criterion 2 resolves from `defaults` alone at each of the six points; criterion 3
adds the `repo_root` argument there (the repository root at the step, the two run commands and the
canary, the `--against` checkout root at the replay, the critic's `self.root`). `SpawnHost` derives no `Default` and its fields are `pub`; `SpawnOpts`
loses its `Default` derive (`agent.rs:82`) and gains `SpawnOpts::new(host: SpawnHost) -> SpawnOpts`,
filling every other field with its former default, so every construction spreads
`..SpawnOpts::new(host)` and the compiler enumerates every site, tests included (a test builds its
host as a struct literal; criterion 3 adds `helpers` to every such literal). A test asserts that no
production line outside the body of `resolve_spawn_host` (the lines from `fn resolve_spawn_host` to
its closing brace at the same indent, exempt) constructs a `SpawnHost` literal (a production reader
accesses the fields, `host.grep_guard` and `host.helpers`, and never destructures a `SpawnHost`, so
every match is a construction): it matches `SpawnHost {` followed, after optional whitespace or a
newline, by a field name or `..`, and never preceded on its line by `->`, `struct ` or `impl `, so
the declaration, the impl headers and the resolver's and the `RunCtx` accessor's signatures are
exempt by that rule. The assertion reads production text through one shared production-boundary reader that criterion 2
OWNS, in `tests/common/fixtures/production_boundary.rs`, by one whole rule: every `tests/` directory (the
repository root's and each `crates/*/tests/`) is excluded; a file declared by a `mod <name>;` whose
attribute run holds a `cfg` predicate that requires `test` (`test`, or `all(test, ...)`) is excluded,
and `any(test, ...)` is production; within a file the cut fires at a `mod <name> {` whose attribute
run holds such a predicate, and every other `cfg`-gated item before it is production. An attribute
run is the contiguous attribute lines in front of the item, a multi-line attribute joined into one,
so `#[allow]` or `#[path]` lines between the `cfg` and the item do not defeat it. The live cases, in
`crates/rigger-conductor/src/lib.rs`: :14-15 (`#[cfg(all(test, ...))] mod blast_radius_eval;`,
excluded), :125-132 (a multi-line `#[cfg(all(test, ...))]` whose run continues through `#[allow]` and
`#[path]` to `mod host_fixtures;` at :132, excluded) and :198-199 (`#[cfg(all(test, ...))]
mod test_support {`, the cut). The reader lives in the shared fixture home beside `language_marker_hits`:
`tests/common/fixtures/mod.rs` declares `mod production_boundary;` and `pub use
production_boundary::*;`, so a test module of the root crate reaches it as `crate::test_support::...`
and a file under `tests/` through `common::fixtures`. Criterion 2 also makes every production-line
cut already in the tree call it rather than spell its own: the production-line pin in
`src/cli/setup.rs`'s test module (`opens_test_module` and the `lines[..end]` cut) imports the reader
through `crate::test_support`, so the cut has one home, and at that moment the pin's cut widens to the same rule (its
text set is read with the same boundary). Criterion 18's sweep is the reader's second caller and does not
define it. Criterion 2 owns `SpawnHost` with its `grep_guard` field, `resolve_spawn_host`,
`SpawnOpts.host`, `SpawnOpts::new`, `Deps::spawn_host`, the `RunCtx` accessor, the guard flag at the six
resolution points (`rigger step`'s once-per-step call before the pin check, `run_cli`'s,
`run_workflow`'s, `cmd_replay`'s, the critic's and the canary's), the edit of every construction and that assertion, and `spawn_config_args`' `host: &SpawnHost`
parameter with both `build_args` callers passing `&opts.host`, so its text holds on a tree without criterion 3;
criterion 3 adds only the `repo_root` argument (at all six points, the replay's `--against`
checkout root included), the `helpers` field, its fold into the hash and both
readers, the `host.helpers` read inside `spawn_config_args` (the `--agents` JSON) and
`allowed_tools_args` taking `&SpawnHost` with its argument at both `build_args` callers (`crates/rigger-driver/src/driver/mod.rs:45-58`, the tool
pre-approval from the same value), so 3 needs 2. Each criterion's test proves the critic's and the
canary's spawns carry the resolved value.

**THE OPERATOR'S HELPER COPY IS THE DEFINITION, decided here (criterion 3).** One function,
`hooks::resolved_helpers(root: &Path) -> Result<Vec<HelperCopy>, Error>`, returns one entry per
`HELPER_AGENTS` helper as `HelperCopy { name, text, present: bool }`: `<root>/.claude/agents/<file>`
with `present` true when that file exists, the embedded text with `present` false only when the
file does not exist; `resolve_spawn_host` calls it once and carries its value as `SpawnHost.helpers`, the one
carrier. `helper_agents_json` and `helper_mcp_tools` (`crates/rigger-driver/src/hooks.rs:117`, :153) take that value:
`spawn_config_args` builds `--agents` from `host.helpers`, and `allowed_tools_args` builds the
pre-approval from the same value; both `build_args` callers, the headless host driver module of
`rigger-driver` and the cli host (`crates/rigger-driver/src/driver/cli.rs:275`, its `allowed_tools_args` call at :290), pass the spawn's `SpawnOpts.host` to `allowed_tools_args` as criterion 2 has them pass it to
`spawn_config_args`, so
the `--agents` JSON and the tool pre-approval derive from one resolution. The root is the repository
root the run drives (for the conductor `Deps::repo`, `conductor.rs:1986`): the operator's checkout is
the one root every spawn reads, and a helper copy committed into a unit worktree is ignored (this
repository tracks `.claude/agents/`, `.gitignore:50`, and `crates/rigger-driver/src/hooks.rs:54` embeds the tracked file, so a
worktree copy is only ever the operator's own text at an older commit); an empty root, the
repo-less path's, is the working directory, the directory the definition pin reads. An existing file that cannot be read or does not parse as a helper definition makes
`resolve_spawn_host` return `Err` naming the file (never the embedded text), an infrastructure fault that charges no
remediation attempt; `rigger validate` reports the same error. `definition_hash` (`src/cli/mod.rs:1142`) becomes
`definition_hash(root, helpers: &[HelperCopy])` and folds only the `present` entries after the agent
definitions. Each of its three callers reads the copies once, at its own root, and passes that value:
`cmd_step` (`src/cli/run.rs:582`) passes the same value to `definition_hash` and to
`Deps::spawn_host`, so there is nothing to assert; `fresh_run_if_requested` (`src/cli/run.rs:1904`)
takes `helpers: &[HelperCopy]`, the slice `definition_hash` already takes, resolved once by its caller
(`run_cli` or `run_workflow`) from the `SpawnHost` it built, so `src/cli/run.rs` spells neither
`SpawnHost` nor `SpawnOpts` and stays outside the prompt-bearing set (the Goal's 22-file, 302-literal
count stands), and `materialize_config_at_rev`
(`src/cli/mod.rs:3094`, at its checkout root) passes the helpers of the replay's one `SpawnHost`, the
value `cmd_replay` also hands `Deps::spawn_host`. An absent copy folds nothing, so the hash of a project without copies is unchanged and it
sees no drift. A mid-run edit of an operator copy is a definition drift that halts the step and
resumes with `--rebase-definition`, as a persona edit does. Criterion 3 owns the `present` marker, the `helpers` parameter of `definition_hash`, all three
callers' reads and their tests. `HELPER_AGENTS` stays as the scaffold's
write-once seed (`setup.rs:244-245`) and the default.

**THE COMMUNICATION CADENCE HAS ONE HOME, decided here (criterion 4).** The cadence clauses are
cut, the shapes and nouns kept: `EMIT_PROTOCOL` reads "Record each decision you make by calling the
rigger_emit tool with type ..." (the clause "the moment you make it" cut); `review_protocol` reads
"Record each review finding you raise by calling the rigger_emit tool with type ..." ("the moment you
raise it" cut); `DECISIONS_HEADER` becomes "Decisions that govern these files:"; `renderPeers` keeps
"PEERS CONTEXT REFRESH - concurrent agents have already decided the following about files in your
blast-radius." and its lines, its two sentences "Do not contradict them; supersede explicitly if you
must." and "Re-check the rigger_peers tool between your own actions to stay aware of decisions made
while you work." cut. Nothing lands: the `communication` built-in already states each cadence ("Record
EVERY decision the MOMENT you make it", the reviewer bullet, "Between your actions, CHECK the
`rigger_peers` tool", "NEVER silently contradict a peer's decision ... SUPERSEDE it explicitly"), so
each cut copy is its duplicate. `PLAN_PROTOCOL`'s clause is criterion 8's. A consumer who switches
`communication` off removes the cadence from every host: intended.

**THE GROUNDING HEADERS AND THE RUN NOTES CARRY NO METHOD, decided here (criterion 4).** Under WHAT
MOVES AND WHAT STAYS's imperative rule, criterion 4 cuts each method clause and keeps each header's
noun: `CODE_NEIGHBORHOOD_HEADER` (`conductor.rs:13455`) loses " - read first", reading "Code
neighborhood of these files (definitions the run extracted into the unified graph):";
`DESIGN_INTENT_HEADER` (:13504) loses ", honor them", ending "and the local rationale - reached by
the one graph traversal):"; the lessons header (`write_capped_lessons`, :14133) loses " (do not
repeat these mistakes)", reading "Lessons already learned about these files:"; the findings header
(`write_capped_findings`, :14158) loses " (address or refute them)", reading "Findings other
reviewers have already raised about these files:"; `review_protocol`'s live sentence loses " and
address or refute it", ending "see it immediately (via grounding and rigger_peers)." Each header
clause's pronoun stands for its header's noun, so the four land as one sentence with that noun phrase
in the pronoun's place, as a slot does, in a new paragraph of the `working-discipline` built-in after
the first paragraph of its `## One pass, by excellence` section: "When your task prompt shows the
grounding of the files you touch, read their code neighborhood first, honor the design intent that
governs them, do not repeat the mistakes in the lessons already learned about them, and address or
refute the findings other reviewers have already raised about them." `review_protocol`'s clause is
cut as that paragraph's duplicate. `notes_block`'s closing (:13283-13285) becomes "Fix every note
that is still true; leave a note that is false or already fixed."; its sentences "Check each note
against the current tree." and "A note owes no record beyond the commit that fixes it." land
verbatim in the `working-discipline` built-in's `## When you review` section directly after
"without one, the notes stay in the run's log for the operator.", and its ", in one pass," is cut
as the duplicate of that section's "that stage fixes the run's notes once". Criterion 4 owns these
cuts, the two landings and their pins: the conductor's header and notes tests,
`tests/common/fixtures/instructions.rs`'s `WHEN_YOU_REVIEW` (nine sentences, the two after its
sixth, which the whole-section pin
`the_working_discipline_body_holds_the_review_method_under_when_you_review`, `src/cli/setup.rs:4080`,
reads), and `tests/critique_method_persona_periphery.rs`'s `ADVERSARY_FINDINGS_PROTOCOL`, which
spells `review_protocol` whole. Every other test reads these headers by a prefix the cuts keep.

**THE PROGRESS CADENCE HAS ONE HOME, decided here (criterion 5).** The wording that lands is the
workflow script's (`rigger.js:676-678`), the superset of the other three, as one bullet of the
`communication` built-in, inserted after its last existing bullet and before its closing sentence
("The exact JSON shapes for these emits are given in your task instructions; ..."), the pin's
expected text being the bullet's text: "- After each significant step - a search, a file read, a build, a
commit, a decision - report ONE short line of what you just did through the progress verb your task
or your tool list names. This is how an observer sees you working between the milestones you record, so a long silent
stretch is never mistaken for a stall. Keep it flowing WHILE you work; do not batch it at the end."
(the slot "from ${REPO}, using Bash" replaced by "through the progress verb your task or your tool list names"). Each
host keeps the verb and its arguments: `progress_framed`
(`crates/rigger-driver/src/driver/mod.rs:63`), the framing both process hosts append after the task,
reads "--- rigger driver ---" then "LIVE PROGRESS: record a progress line {report}.", each host's verb
filling `{report}` as at the base: the cli host's `framed` (`crates/rigger-driver/src/driver/cli.rs:146`)
"by running (Bash): rigger progress '<id>' '<one line: what you just did>'" and the headless host
driver module's `framed` "by calling the `mcp__rigger__rigger_progress` tool with that line as its
activity" (`SPAWN_PROGRESS_TOOL`); the script's `progressNote` reads "LIVE PROGRESS: record a progress line, from ${REPO}, using Bash:"
then the command line; the shim's `progressNote` (which has no `REPO` binding) reads "LIVE PROGRESS: record a progress line
by running (Bash):" followed by the `rigger progress '${next.id}' '<one line: what you just did>'`
line, as the cli host's verb does; the `rigger_progress`
description reads "Record one live progress line for the spawn this server is bound to, so an observer
sees you working between milestones." ("Report one short line after each significant step; never
batch them up." cut). Criterion 5 owns extending the `communication` body pin
(`the_third_built_in_is_the_communication_discipline_verbatim`,
`crates/rigger-domain/src/instructions.rs:337`) to the added bullet. The heartbeat's "after each
significant step" (`rigger.js:666`, :669) stays: the liveness sweep reads the marker (CONTRACT).
The spec critic's task carries the headless host's frame as every launch does, and
`CRITIQUE_FRAMING` (`crates/rigger-domain/src/review.rs:325-331`) voids recording through
`rigger_progress` for that task; both stay.

**A DOC-ONLY ROUND STATES THE FACT, decided here (criterion 6).** `DOC_ONLY_ROUND` becomes "This round
changed documentation only." `is_documentation_only` (`conductor.rs:14704`) stays as the fact's
classifier. The rule "Extend the existing owner test's needle for the changed sentence, or add no pin;
never a new test." (the clause after the colon, capitalized as a sentence) lands verbatim in
`.rigger/agents/sdet-author.md` under a `## A documentation-only round` heading, the
file's last section, after `## Every test asserts exactly`. The constant's doc loses "The rule lives here alone, never copied into a persona." The
scaffold seeds no sdet-author persona and declares no test author (SCAFFOLD DEFAULTS), so this
repository's persona is the rule's one home; a consumer declaring a test author writes its own rule.

**THE UNIT SIZE CAP IS DECLARED, decided here (criterion 7).** `Defaults` gains
`max_unit_added_lines: u32`, default 0 (no cap). The key lives under `defaults` only: the planner and
the plan-critique gate both read it, and one home serves both. `MAX_UNIT_ADDED_LINES` is deleted;
`unit_size_cap(cap: u32) -> String` renders its first sentence from the key, "A unit is too large
when it is expected to add more than {cap} lines." (FRAMING: the declared number), in place of the
constant-based sentence and never beside it, and is called only when `cap > 0`. In both prompts that
render it, the DAG critique prompt (its opener's cap paragraph, `conductor.rs:8380-8386`) and `PLAN_PROTOCOL` (its
`{unit_size_cap}` slot, rendered as that sentence followed by a space), the cap sentence appears only
when the key is declared; absent, neither prompt carries a cap sentence. The function's second
sentence, "The cap measures review scope - how much one review round must read - never code shape."
(`conductor.rs:13187`), leaves the binary and lands verbatim in this repository's
`.rigger/agents/planner.md` and the scaffold's planner seed, its one home, directly after criterion
8's moved split sentence in both homes (so 7 needs 8). Because the sentence reaches the plan
critique today through the DAG prompt's cap paragraph, criterion 7 also lands it verbatim in the
adversary's and the adjudicator's `## When you critique a spec or a plan` sections, persona and
scaffold template alike, appended after a space to each section's `- Unit size:` bullet, otherwise
untouched (`.rigger/agents/adversary.md:34`, `.rigger/agents/adjudicator.md:43`, the templates at
`src/cli/setup.rs:2020`, :2079), so no persona this repository or the scaffold ships loses it. Three
role-voiced homes are not a duplicate under the one-home rule: the home of a method is the persona of the role that applies it. The DAG prompt's cause-contract example "a unit over the
size cap" is rendered only when the key is declared: `DAG_CRITIQUE_CONTRACT` gains a `{size_cap_cause}`
slot (`conductor.rs:8438-8439` at the base), filled by the CONTRACT const `DAG_SIZE_CAP_CAUSE`, "a unit
over the size cap, ", when the key is declared and by the empty string when it is absent; criterion 7
owns the slot and its fill, criterion 16 the const's row. The binary keeps the `decomposition-conflict`
cause contract. Criterion 7 owns every pin of `unit_size_cap`'s text and of `MAX_UNIT_ADDED_LINES`
(`the_plan_protocol_and_the_dag_critique_carry_one_unit_size_cap`, `conductor.rs:49727`, and
`UNIT_SIZE_CAP_SENTENCE`, `tests/common/fixtures/plan_critique.rs:33`, among them). `SCAFFOLD_WORKFLOW`
declares `max_unit_added_lines: 3000` (SCAFFOLD DEFAULTS); this repository's workflow declares 3000
(OPERATOR HAND-OFF).

**THE PLANNER PROTOCOL CARRIES NO POLICY, decided here (criterion 8).** Every `PLAN_PROTOCOL`
sentence (`conductor.rs:13202`) is in exactly one of two lists, by one rule: a sentence stating what
the conductor does with the planner's output is FRAMING and kept; a sentence telling the planner how
to plan moves.

KEPT (FRAMING), each as at the base unless reworded here: "You are the planner. The conductor has ALREADY
created one baseline implement unit per acceptance criterion below - the spec is decomposed by
construction."; "Your job is to REFINE that baseline, not to re-decompose it." (`PLAN_ROLE` ends with it and one
space, so the `{unit_size_cap}` slot after it, the cap sentence and a space or the empty string,
leaves exactly one space before the next sentence on both branches); the split bullet's
contract, reworded as its own sentence after the `{unit_size_cap}` slot: "Each part of a split echoes
the split criterion's id and ends its `criterion` text with an OWNS sentence naming its part (the conductor
keeps that sentence after the criterion's exact text as that unit's contract)."; "Each criterion below
is shown with a STABLE id in [brackets]."; "When your unit serves a criterion, your unit SUPERSEDES
(replaces) that criterion's baseline - it does NOT run alongside it - so identify the criterion you
serve by ECHOING its id: ..." (to the end of its parenthetical); "The conductor matches your unit to its
baseline by that id, ..." reworded whole (MARKERS: "matches" becomes "pairs your unit with its
baseline") to "The conductor pairs your unit with its baseline by that id, so even if you reword or
truncate the criterion text in `criterion`, the correct id still supersedes the one baseline (no
duplicate)."; "A wrong or missing `criterion_id` is what makes your unit run as an EXTRA
unit on top of the baseline (duplicated work)."; "Still copy the criterion text into `criterion`
(verbatim is safest)."; "Several units echoing the SAME id (a real split) all run and replace the one
baseline."; "Propose each refinement by calling the rigger_emit tool with type \"UnitProposed\" and
data:" with its JSON shape (the cadence clause "the moment you decide it" cut); "A unit whose
`criterion_id` matches none of the ids below still runs, but as a genuinely-new sub-unit that the
conductor flags as unmatched." (its clause "- so only omit the id when you truly intend a new
sub-unit" cut and moved; FRAMING of
`conductor.rs:12907`; MARKERS: "matches none of the ids below" becomes "is none of the ids below");
"Every unit you propose - a refinement, a split, or a new sub-unit - automatically runs the fan-out
template's own gates; you never need to name them."; and the closing "The acceptance criteria to
refine against (...):" line with `{criteria}`.

MOVED (planning policy), each verbatim to this repository's `.rigger/agents/planner.md` and the
scaffold's planner seed, appended once in this order: "Split a criterion whose one unit would be too
large into ordered units (each `needs` the one before it), each owning a named part."; "If you
discover a NECESSARY sub-unit or an ordering dependency the baseline missed, propose it."; "NEVER
propose a unit that maps to no acceptance criterion - that is scope creep."; "Propose each refinement
the moment you decide it."; "Do not write code." (the seed already carries it, so only this
repository's persona gains it); "Only omit the id when you truly intend a new sub-unit." (the clause
cut from the unmatched-id sentence, capitalized). The first is the split bullet's policy clause cut at "each owning a
named part"; the fourth is the cadence clause cut from the `UnitProposed` sentence, with its verb
phrase as subject.

**THE REQUIRED PATTERN NAMES ITS RECORD, decided here (criterion 9).** `PriorFailure::block`'s PATTERN
line (`conductor.rs:1418-1423`) becomes "  PATTERN: {shape}. Record DecisionMade `{audit}` for it."
and `ReviewRound::block`'s (:13325-13328) "  PATTERN: {shape}. The implementer's audit record is
DecisionMade `{audit}`." No bullet is added: the method's home is the `working-discipline` built-in's
existing bullet "A whole-tree audit a rejection requires is bounded." in its `## Hand over a tree the
gates only confirm` section (`crates/rigger-domain/src/instructions/working-discipline.md:73-77`),
whose fix-every-site sentence and Check sentence state it; the cut clauses "listing every site
checked" and "check it names every site" are the unbounded form that bullet bounds. Criterion 9 owns a
pin that the bullet stands verbatim. A later review round holds the `{audit}` record to that
bullet's Check sentence, which reaches every reviewer's system prompt through
`instructions::compose`, so no reviewer text is added. No code check is added.

**THE TEST AUTHOR IS DECLARED, decided here (criterion 10).** `Defaults` gains `test_author: String`,
default empty. `spawn_sdet_author` reads `self.cfg.agents.get(&cfg.workflow.defaults.test_author)`
when the key is non-empty and returns `Ok(())` (no spawn, no budget slot) when it is empty, whatever
agents `.rigger/agents` defines. The spawn id keeps the `sdet-author` role token
(`spawn_retry_id(stage, ROLE_SDET_AUTHOR, attempt, retry)`), so recorded ids, session resumption
(`continue_prior_session`) and the cache choice at :4811 are unchanged; only the agent definition is
the declared one. `Config::validate` refuses a non-empty `test_author` naming no defined agent,
naming the key and the id, as it refuses a stage agent. Criterion 10 owns every existing test whose
fixture relies on the fixed-id convention (the sdet tests at `conductor.rs:32191`, :41041, :48300,
:48407, :48585, :48676, :48769, :48875, :48987, :50584 and `tests/unified_traversal_grounding.rs:630`), adding `test_author` to its
fixture. The doc of `spawn_sdet_author` (`conductor.rs:5481-5483`, "The fixed `sdet-author` id is a convention ...
no `workflow.yml` change") is rewritten to the key. `SCAFFOLD_WORKFLOW` declares none (SCAFFOLD
DEFAULTS); this repository's workflow declares `test_author: sdet-author` (OPERATOR HAND-OFF).

**THE WORKFLOW SCRIPT NAMES NO TOOLCHAIN, decided here (criterion 11).** `WaveItem` (`spawn.rs:849`)
gains `build_cache_env: String`, always on the wire and non-null on every wave item (it is the
constant, decoupled from the cache path), its value the one constant `spawn::BUILD_CACHE_ENV`, which
criterion 11 adds (it does not exist at the base) and makes `RunCtx::spawn_env` (`conductor.rs:8966`)
and both gate-runner sites (`ExecRunner::run`, `crates/rigger-gates-shell/src/gate.rs:608`, :656)
and `spawn_request`'s env lookup (`crates/rigger-domain/src/agent.rs:213-218`, which fills the
request's `cargo_target_dir`) read, value unchanged, the variable's name having one home. `RunCtx::spawn_env` keeps its shape at
the base and gains no argument: it sets `spawn::BUILD_CACHE_ENV` to `worktree::unit_sibling(dir, cache)`
with the `cache` argument unchanged (the review tier keeps its review-cache sibling;
`conductor.rs:8963-8969`) when `dir` has one (`unit_sibling`, `crates/rigger-domain/src/spawn.rs:47`,
which recognises only `rigger-wt-` dirs) and otherwise adds NO cache variable. A spawn with no unit sibling builds where its working tree's
toolchain defaults: `re_plan`'s planner runs in the project cwd (`conductor.rs:8519`, `dir:
String::new()`), so its builds land in the operator checkout's own build directory; a reviewer in the
check-in stage's throwaway review-only worktree (`run_fan_out_stage`, `review_only_worktree`, torn
down on every terminal exit, kept across a park) builds inside that worktree, and the teardown removes
it with the worktree. No new cache class is added: the shared gate cache is reaped whenever its guard
is uncontended (`gate.rs:634-657`; `reclaim_shared_build_cache`, `src/cli/mod.rs:4154`), and review reproductions never share
a cache with gates (`spawn_env`'s doc, `conductor.rs:8957-8962`). Criterion 11 owns the no-sibling
case and its test: a spawn in a review-only worktree, and the planner at an empty dir, carry no cache
variable; a unit spawn carries its sibling. The reviewer's build-cache sentence follows the same rule:
criterion 11 makes the sentence "A reproduction builds into the build cache your spawn carries:
the unit's review cache, never the cache the unit's gates use." its own FRAMING const,
`REVIEWER_BUILD_CACHE`, its text opening with one space as `REVIEWER_DISCIPLINE`'s does, and `review_protocol` (`conductor.rs:13227`) gains a `carries_cache: bool`
argument and appends that const after `REVIEWER_DISCIPLINE` only when it is true (at the base the
sentence is the middle one of the const's three; `REVIEWER_DISCIPLINE` keeps the other two, adjacent
and in base order, so a spawn carrying a cache reads it after them, the one reordering this criterion
makes); every caller (the
tests included) passes whether the spawn's environment carries `spawn::BUILD_CACHE_ENV` (the wave
item's cache path non-empty), and the adjudicator prompt's direct append (`conductor.rs:8276`) follows
the same rule. A tier in a unit worktree gets the sentence; a reviewer with no unit sibling (the
check-in stage's review-only worktree, the plan-critique reviewers, `conductor.rs:8743`, and the
canary's reviewers, whose environment is empty) gets none, and the other discipline sentences render
verbatim and unchanged in both branches; the pin of the constant's three sentences
(`every_reviewer_prompt_carries_only_the_contract_and_its_system_prompt_the_method`,
`conductor.rs:41148`) reads the two consts together in that order, its sentences unchanged. A no-sibling reviewer's reproductions build, per
SCRATCH POLICY, in its throwaway scratch worktree under its registered scratch path, reaped once its
result records; a build it runs in place in the review-only worktree lands in that worktree's
toolchain-default directory and is torn down with the worktree. Criterion 11 owns the conditional
render and its two concatenation fixtures. The script renders BUILD LOCATION conditionally at the base
(`rigger.js:684-688`, `req.cargo_target_dir ? ... : ''`, the wave field null when the request's path
is empty, `spawn.rs:931-934`), so a wave item with no cache path renders no BUILD LOCATION block and
its sentence "A build cache inside the worktree itself is a defect that fills the disk - never
create one." is rendered only with a cache path; criterion 11 keeps that conditional. The
script's STEP schema (`rigger.js:195`, :203) lists `build_cache_env` as required, type `'string'`, beside `cargo_target_dir`. The wire field `WaveItem::cargo_target_dir` (`spawn.rs:895`) keeps its name as CONTRACT: it is
the wire's field name, read by `${req.cargo_target_dir}` interpolations, never prompt text (Global
constraints). The constant's value is the toolchain coupling OUT OF
SCOPE names; the script's text names none. The rewordings: `workdir` (`rigger.js:650`) "Do all your
file edits, your build tool's runs, and any git commit inside your isolated worktree ..."; BUILD
LOCATION (:685-687) "every command your build tool runs inside your worktree (anything that builds or
tests) MUST run with" then "  export ${req.build_cache_env}='${req.cargo_target_dir}'" then "set
first, in the same shell. That directory is the build cache the conductor chose for this spawn - ..."
(the rest as at the base, except its last sentence, which becomes "A build cache inside the worktree itself
is a defect that fills the disk - never create one."); SCRATCH POLICY (:697-699) keeps "NEVER under /tmp or your own session scratchpad"
and the reaper sentence, drops the parenthetical "(those are on the operator's small OS partition, and
a single cargo target or `rigger setup` shim install there fills the disk)", and its next sentence
reads "For any build you run outside your assigned worktree, export your build cache variable
(`${req.build_cache_env}`) to a subdir of that same printed path first - never the shared build
cache.", rendered for every spawn as at the base; its reaper
sentence becomes "It is reaped once your result records - do not store anything durable there." (the
rest of the sentence verbatim; criterion 11 owns the rewording, since "the moment" is a phrase-list
entry). The step courier (:918-920) runs `cd ${REPO} && rigger step --spec ...` with no cache prefix,
drops the sentence "(the CARGO_TARGET_DIR prefix makes every gate share one build cache ... - keep it
exactly as written)", and its timeout clause reads "a step integrates ready units by running their
gates inline and can take 20+ minutes ...". The prefix goes because every gate and regenerate build gets its
directory from the binary (`shared_build_cache_paths`, `conductor.rs:9010`, its callers :9121, :9680,
:10828, and the per-unit caches) and a spawn gets one exactly when it has a unit sibling; a spawn
without one inherits the ambient environment as at the base, minus the prefix. The degraded gate branch
(a guard requested whose probe failed, `crates/rigger-gates-shell/src/gate.rs:634-657`) today
withholds the variable and falls back to the ambient one, which only the courier's prefix fills
(`${REPO}/.rigger/tmp/cargo-target`, outside the scratch root, whose default is the cache home). The courier prefix drop lands only after the failed probe halts the step (criterion 19, THE FAILED
GUARD PROBE HALTS THE STEP): dropping the prefix leaves the degraded branch no ambient fallback, so 11
needs 19. A gate run that requests no guard and names no directory inherits the ambient environment;
no production caller does this (every conductor call hands `build_cache_dir` whenever `target_dir` is
empty): the accepted residue, its behaviour unchanged, and no criterion owns a test for that case. So
the courier passes no cache prefix.

**THE FAILED GUARD PROBE HALTS THE STEP, decided here (criterion 19).** A failed guard
probe (a guard requested whose probe failed) is an INFRASTRUCTURE FAULT, not a gate failure:
`ExecRunner::run` returns it as a distinct runner return, an error variant of the gate runner's
result (`Runner::run`, `crates/rigger-gates-shell/src/gate.rs:69`, returning `Result<GateResult,
ProbeFault>`, the fault carrying the probe error: the guard path and the OS error), never a
`GateResult` with a `pass` value, and runs and builds nothing anywhere: not into the gated tree, the shared cache (withheld from an
unguarded build on purpose, `gate.rs:634-654`) or the ambient
directory. `run_gates_at` (`conductor.rs:9057`) propagates the fault before building any
`GateOutcome`, so it bypasses the F3 ordinal rerun (`conductor.rs:9054-9056`); verdicts already
recorded in the same pass stay and replay on the resumed step. Every `Runner::run` caller passes
`ProbeFault` up the same way: the inline calls in `run_gate_with_taxonomy` (`conductor.rs:9339-9421`), which returns `Result`, so a
fault on its first run or on any taxonomy rerun propagates before any verdict and `run_gates_at`
passes it up, the deferred
`run_deferred_gates` (:9681-9691) and the regenerate `run_regenerate_command` (:10822-10860), each
recording no verdict, the regenerate returning no `Err` of its own. `impl Runner for ReplayRunner`
(`src/cli/mod.rs:3133`) never returns `ProbeFault` (always `Ok`); criterion 19 owns it alongside
`ExecRunner`. `ProbeFault` travels as the conductor's `Error`: `run_gates_at` converts the runner's
`Err(ProbeFault)` with `?` into an `Error` carrying a `PROBE_FAULT_MARKER` sentinel, the shape every
conductor halt signal has (`Error` is a message newtype; markers are read by `carries_marker`,
`conductor.rs:1507`), and `PROBE_FAULT_MARKER` joins `HALT_MARKERS` (`conductor.rs:1852-1858`, whose
doc says a new halt joins the list and never grows an arm of its own): one halt mechanism, not two.
It joins neither the run's hold (`RunHeld`, `crates/rigger-domain/src/ledger.rs:229`), which parks the
run on a cause outside the machine until a probe finds the cause cleared, nor the `SPAWN_CRASH_MARKER`
rerun under the F3 infra retry: a failed guard probe is a local filesystem answer that no wait and no
rerun changes, only the operator's fix.
The inline and regenerate callers unwind through `run_single_stage` (`conductor.rs:5766`) and
integration (the regenerate caller is reached from `regenerate_conflicted_paths`, :10893, through
`regenerate_and_record` and `catch_up_owed_regeneration` from `integrate_and_emit`, inside
`run_stage`, inside `run_wave`) to `settle_stage`'s existing `HALT_MARKERS` arm
(`conductor.rs:4423-4439`), which handles it exactly as the other infrastructure halts: no attempt
charged, no lesson, no verdict for the faulted gate, the pass's earlier verdicts kept, the halt kept
as the wave's first error with the marker stripped, and `conductor::run` returning that `Err`. The
deferred caller, `run_deferred_gates` (`conductor.rs:9597`), runs at the phase boundary after the
wave loop; its `?` at :2684 already propagates its `Err`, recording no deferred verdict and no
`DeferredGateFailed` because nothing after the `?` runs. Before it returns that `Err`,
`run_deferred_gates` strips the marker through the fold `settle_stage` applies (`conductor.rs:4436`),
moved into one function both call, so the deferred halt reaches `conductor::run`'s caller with the
same message as the inline halt. A `HALT_MARKERS` `Err` surfaces today as
the command's own error: `cmd_step` returns it from `conductor::run(&cfg, &deps)?`
(`src/cli/run.rs:879`) before printing any wave, and `main` prints `rigger: <message>` on stderr and
exits 1 (`src/main.rs:194-202`); `run_cli` (`src/cli/run.rs:1642`) surfaces it the same way, and the
workflow driver's courier reports the failed step command as it does for any halt. The probe fault
surfaces the same way, its message carrying the guard path, the OS error and the operator step.
No production regenerate requests a guard: it runs in a unit worktree, whose cache sibling is set, so
`run_regenerate_command` (`conductor.rs:10823-10831`) passes an empty guard and the runner never
probes; its `?` is the type's, and no resume is decided for that path. A fault in the post-merge re-gate
(`run_gates` under `GateSelection::PostMerge`, `conductor.rs:10436`) leaves the landed merge
in place: the LANDED row (`RunCtx::landed`, :3281, seeded from the `integrate-landed` records)
carries the resume, whose `integrate_and_emit` re-runs the re-gate before `UnitIntegrated`, and units
that land on it while the halted wave finishes are kept, as at the base for any post-merge `Err`.
Retention follows the `HALT_MARKERS` route at every teardown site, with no special keep and no
extension of `is_parked_or_budget_refused`: a halt is not a parked unwind, so `run_stage`
(:4647-4651) removes the unit's worktree and, through `Worktree::remove`, its per-unit cache, and
keeps the unit's branch (deleted only on a successful integrate, :4661-4662); `run_fan_out_stage`
(:7381-7391) and `run_plan_critique_gate` (:8605-8610) remove their throwaway review worktree and its
branch. Criterion 19 owns the marker's membership in `HALT_MARKERS` and its tests: a forced fault in
one unit's inline gate ends the wave through `settle_stage`'s `HALT_MARKERS` arm with that `Err`,
charging no attempt and recording no verdict while the pass's earlier verdicts stay; a forced fault in
a deferred gate ends `conductor::run` with the same `Err` and records no deferred verdict; a stub
runner's fault on the regenerate command returns the same `Err` from `regenerate_conflicted_paths`. So no remediation attempt is charged, no `UnitFailed` and no `PriorFailure` evidence is written, and no
gate verdict is recorded. The probe (`build_cache_guard_is_usable`, `gate.rs:729-748`) first creates
the guard file's parent directory with `create_dir_all` before opening the guard, so a missing scratch
root is never a fault and only a real fault (the OS refusing the create or the lock) halts. The halt
text names the guard path, the OS error and the operator step: fix the path or set the project's
scratch root to a filesystem with advisory locks, then relaunch; the halt charged no attempt and
recorded no verdict, so the relaunched step probes again. Criterion 19 owns the create and its test
(a probe over a missing parent directory is usable; a probe whose parent cannot be created returns
the fault). The base's degrade tests turn with it: `exec_runner_degrades_to_unguarded_when_the_guard_path_cannot_be_opened`
(`gate.rs:1196`) becomes the missing-parent test, asserting the guard file is created, and
`tests/reset_build_cache_periphery.rs`'s degraded-guard test (:463) forces a parent that cannot be
created (a path under a regular file) and asserts the `ProbeFault`, that the command never ran and
that nothing was written into the shared cache. The guard's parent directory is not a build directory and may remain after a failed
probe. Criterion 19 owns the branch, the `ProbeFault` return, its propagation at
all three callers and their tests: a forced guard-probe failure returns the distinct return with the
probe error, builds no `GateOutcome`, runs no F3 rerun, records no gate verdict while keeping the
pass's earlier verdicts, ends the step with the halt `Err` naming the error and leaves no build
directory behind, at each caller.

The script's comment lines are not prompt text and stay. Criterion 11 edits the pins of
`shipped_workflow_driver_tells_a_worker_its_units_build_location` (`src/cli/mod.rs:5923`) that quote
the old text.

**THE SCRATCH TOOL NAMES NO TOOLCHAIN, decided here (criterion 12).** The `rigger_scratch`
description (`crates/rigger-dash/src/mcpserver.rs:786`) reads "Your own rigger-assigned scratch container - the one directory
every probe repo, verification worktree, test build, or build cache you create must live under, never
/tmp or a session scratchpad. Reaped once your result records." ("Reaped the moment your result
records." reworded, since "the moment" is a phrase-list entry; criterion 12 owns the rewording).

**THE PLANNING SKILL CARRIES THE PROJECT'S CONSTRAINTS, decided here (criterion 13).** The repository
skill home is the operator overlay, not a forked skill: `DocsOverlay` (`src/cli/mod.rs:4918`)
gains `global_constraints: Option<Vec<String>>`, applied onto a new `DocsContext` field (empty by
default). `planning-a-spec` becomes `DocBody::Rendered` (its body text unchanged elsewhere), step 6
titled "**6. Carry the project's own Global constraints.**" followed by "State them once in the spec;
every unit inherits them: no new event type unless the spec's whole point is one; fallback stated for
any criterion that might be impossible; anything the gates cannot see flagged for the adjudicator to
demand evidence on." and, when the overlay lists any, "This project's:" followed by one `- ` line per
entry. The path examples become neutral: "(`path/to/file:line`, event counts, durations)" for
"(`file.rs:line`, ...)" and "(`run_wave`, `src/ingest`)" for "(`run_wave`, `crates/x/src/ingest.rs`)".
This repository's `.rigger/docs-overlay.yml` (new, `global_constraints` only) lists "Hyphens not em
dashes (U+2014 fails the diff gate)." and "Both feature lanes green." `rigger docs` renders the
committed `skills/planning-a-spec/SKILL.md` from the code-derived context without the overlay
(`write_docs`, `src/cli/mod.rs:5395`), so the committed source carries the generic step and the
installed copy carries this repository's list; the unit re-renders the committed source with the
tree's binary. Criterion 13 owns every existing test reading `PLANNING_A_SPEC_BODY` as a static body
(`docs.rs:1870` among them) and the `spec-preflight` test doc's "like `planning-a-spec`'s, ignores
`ctx`" (:2422).

**SKILL SHADOWING, decided here (criterion 14).** `install_skills` (`src/cli/mod.rs:4968`) checks, for
each registry entry, `<root>/.rigger/skills/<name>/SKILL.md`, `<name>` equal to the entry's name byte
for byte (case-sensitive, never trimmed). Present: that file's bytes are installed verbatim at
`.claude/skills/<name>/SKILL.md` (no overlay, no stamped prohibition: the project's copy is the
definition), written only when the installed file differs, and the outcome is `ProjectCopy` on every
setup while the shadow is in place (a state, not a write). `install_skills` returns `SkillInstall {
outcomes: Vec<(&'static str, InstallOutcome)>, strays: Vec<String> }`, `InstallOutcome` gaining the
`ProjectCopy` variant and `strays` carrying each stray's path. Absent: the shipped render installs as at the base. An unreadable project copy (a
`.rigger/skills/<name>/SKILL.md` that exists but cannot be read, or is a directory) fails `rigger
setup` naming the file and never installs the shipped render, the same rule as THE OPERATOR'S HELPER
COPY's malformed copy. A stray is every entry under
`.rigger/skills/` that is not a directory named by a registry entry and holding a `SKILL.md`: a
registry-named directory with no `SKILL.md`, and a plain file or a directory naming no registry entry
(a near name included). A stray shadows nothing and installs nothing, and `rigger setup`'s report lists
both cases from `strays`, each with its path, as shadowing no shipped skill. `cmd_setup` prints the
stray lines before its silent no-op return, so a stray is listed on every setup, and reads
`ProjectCopy` as `AlreadyCurrent` at that return's check and in every `InstallOutcome` match (the skill
loop prints nothing for it). `rigger docs`, the docs-drift
gate and the pre-commit block read only the committed `skills/` sources and the shipped render, never
`.rigger/skills/`, so a shadow is never drift. An installed skill an operator edited by hand with no
project copy is rewritten as at the base; keeping an edit means copying it under `.rigger/skills/` first.

**THE IMPLEMENTER COMES WITH THE GATE SET, decided here (criterion 15).** `SetSpec` (`setup.rs:1840`)
gains a required `implementer: String`: a persona in markdown-with-frontmatter form, its `id` read
from the frontmatter. `scaffold/rust/set.yml` carries the base's `rust-engineer` seed text verbatim under
it. `SCAFFOLD_AGENTS`' `rust-engineer.md` entry is replaced by `implementer.md` (id `implementer`),
the base seed's text with "in idiomatic Rust" cut, the language-neutral default. `SCAFFOLD_WORKFLOW`'s
two `agent: rust-engineer` lines become `agent: @IMPLEMENTER@`, which `scaffold_workflow` fills with the
matched set's implementer id or `implementer`. `init_project` adds the set's persona, as
`<id>.md`, to the seed candidates and writes every candidate, the `implementer` seed included, under
the same referenced-ids filter as the other seeds (`setup.rs:197-212`, `write_if_absent` unchanged): a
matched set's workflow references the set's id, so `implementer.md` is written only when no set
matched. `parse_template_set` refuses, naming the set, an implementer persona that does
not parse and one whose frontmatter `id` equals any `SCAFFOLD_AGENTS` id. The unwired `mutation.sh` and its `set.yml` comment stay: a project gate form. The
`SCAFFOLD_AGENTS` doc ("The ids match this project's own canonical personas (planner, rust-engineer,
...)", "implementer ... deliberately do NOT appear") and `authoring-loops.md`'s checklist step 3 ("your
language's engineer instead of `rust-engineer`") are rewritten to the set's implementer. Criterion 15
owns the seed-set test at `setup.rs:3180-3197`: its canonical set names `implementer` in place of
`rust-engineer` for the no-set render, and its stray list drops `implementer`, which is now a real
seed rather than a stray.

**SCAFFOLD DEFAULTS, decided here.** The scaffold reproduces the base's behaviour for a fresh project
where the base gives every project one: `grep_guard: true` (the base guards every project) and
`max_unit_added_lines: 3000` (the base caps every project); no `test_author` (the base spawns one only for a
defined `sdet-author`, which the scaffold never seeds). Each key carries a one-line comment in
`SCAFFOLD_WORKFLOW` naming what it switches.

**THE REGISTRY, decided here (criterion 16).** The domain crate owns the registry TYPES:
`rigger_domain::prompt_surface` (`core` lane, no I/O) defines `pub struct Group { pub surface:
&'static str, pub name: &'static str, pub text: &'static str }` (every field `pub`, as `SpawnHost`'s
are) and `fill`. A group is one sentence group of one class, CONTRACT or FRAMING, the author's
decision recorded in THE CLASS TABLE below; no code reads the class, so no type carries it: each
group's const doc comment opens with its class word (`/// CONTRACT: ...` or `/// FRAMING: ...`), read
by reviewers and by the graph's doc attribute. Its text may carry `{slot}` placeholders. `fill(text, &[(slot, value)])` scans the template once, left to right, and
replaces only the named `{slot}` tokens it is passed; an inserted value is never rescanned, and every
other brace is literal (the JSON shapes in `EMIT_PROTOCOL`, `REQUIRED_PROTOCOL` and `PLAN_PROTOCOL`
included). Each crate keeps its own groups in a `prompt_surface` module of its own, their one home,
exposing `pub fn prompt_groups() -> &'static [Group]` ordered by surface then by position: the domain
crate only the texts it renders itself (the critique prompt's framing, `CRITIQUE_OUTPUT_CONTRACT`
and `CRITIQUE_RECORDED_FINDINGS`, `review.rs`; and `INSTRUCTIONS_HEADING`, the built-in layer's
heading "# Rigger instructions", moved out of `instructions::HEADING`, `instructions.rs:61`, and read
by `compose`, by `render` (the operator's `rigger instructions` output) and by the prime surface's
first group, the test fixtures reading `rigger::instructions::HEADING`
(`tests/common/fixtures/instructions.rs:240`, :343, :365) re-pathed to it; `compose` gets no
concatenation test, the rest of its output being persona, built-in and operator text no group holds); `rigger-conductor` the conductor's and the canary's
texts; `rigger-driver` the process hosts' framing, report verbs and resume note; `rigger-dash` the MCP
tool descriptions and their `inputSchema` property descriptions
(`crates/rigger-dash/src/mcpserver.rs:781-798`); the binary crate (`src/`)
`GREP_GUARD_MESSAGE`, the write-guard deny reason and the
SessionStart `rigger prime` output's fixed texts (its headers and labels, never the store data they
frame), whose renderer (`cmd_prime` and `instructions_in_force_line`, `src/cli/setup.rs:1595-1677`)
criterion 16 moves into a new `src/cli/prime.rs`, so `setup.rs` stays outside the sweep. The critic's
framing is the domain's `CRITIQUE_FRAMING`, rendered by `review::spec_critique_prompt`
(`crates/rigger-domain/src/review.rs:360-378`); `src/cli/critique.rs` renders no prompt text of its own
and holds only operator messages (:57, :101, :196, :232, :236, :239, :246, :305, :355), allow-listed under
THE SOURCE SWEEP. No adapter text moves inward across a crate
boundary: the dependency rule holds for data as for code, and the domain never spells a host's
vocabulary. Each distinct text is one `pub const` of its crate's module, and every renderer formats
from those constants through `fill`, so no prompt literal stays in the renderer. A text shared by
several surfaces (`REVIEWER_DISCIPLINE`, read by `review_protocol` and the adjudicator's prompt, among
them) is one const that each surface's group references: the duplicate check keys on (surface, name),
and the one-home rule holds on the const, so a text spelled twice as two literals is the defect and
two groups referencing one const is not. The surfaces: `EMIT_PROTOCOL`, `PLAN_PROTOCOL`,
`unit_size_cap`, `REQUIRED_PROTOCOL`, `REVIEWER_DISCIPLINE`, `review_protocol`, `DECISIONS_HEADER`,
`CODE_NEIGHBORHOOD_HEADER`, `DESIGN_INTENT_HEADER`, the lessons and findings headers, the elision notes
and `graph_around_recovery`, `task_block`, `PriorFailure::block`, `ReviewRound::block`,
`gate_evidence_block`, `conflict_resolution_prompt`, `later_attempt_block`, `DOC_ONLY_ROUND`,
`write_peers_pointer`, `write_lookup_pointer`, the DAG critique prompt and `re_plan`'s framing, the
critique prompt's framing, `CRITIQUE_OUTPUT_CONTRACT` and `CRITIQUE_RECORDED_FINDINGS`, the canary's
`review_header` and `adjudicator_prompt` (`canary_store.rs:647-680`), `progress_framed` with each
process host's report verb and the headless host's resume note (`interruption_note`),
`UPHELD_PROTOCOL`, the run-notes block (`notes_block`), the refused-record note
(`RefusedRecord::task`), `RECORD_RECOVERY`, `rigger prime`'s output, the MCP tool descriptions of all
three surfaces and their `inputSchema` property descriptions (moved out of the `json!` literals in
`tool_list`, `crates/rigger-dash/src/mcpserver.rs:778-801`) and
`GREP_GUARD_MESSAGE` (defined in `src/cli/mod.rs:5526`, so its group is the binary
crate's, within its crate) and the `rigger guard-write` deny reason (`guard_write_deny_outside_first_root`,
`src/cli/guard.rs:415-419`, FRAMING: the root the write falls outside of) with `rigger guard-write`'s
exit-2 stderr texts (THE CLASS TABLE's hook rows). Every crate that renders one
already depends on `rigger-domain`. Each HIT ENUMERATION row names its owner; criterion 16 adds no
rewording; criterion 17 only asserts. A text moves only within its crate, into that crate's
`prompt_surface` module, which is its one home; the renderer reads the group. Criterion 16 re-paths
every reader, among them the shipped-text helper (`every_shipped_agent_text`) and the tests that read
`conductor::REVIEWER_DISCIPLINE`, which then read `rigger_conductor::prompt_surface::REVIEWER_DISCIPLINE`. The two
continuation imperatives ("Finish and report; do not start over.", "Fix exactly these - do not start
over:", `conductor.rs:1392`, :1402) are printed by `PriorFailure::block` alone, so they sit inside its
`PRIOR_HALTED` and `PRIOR_RETRY_HEADER` consts, no other group shares them and no separate
continuation const exists; they stay FRAMING;
`CRITIQUE_OUTPUT_CONTRACT` is one const of one class, CONTRACT, since the binary parses the finding
line's shape, its remedy field included. `HELPER_AGENTS`, the scaffold seeds, the skills and the
built-ins are the consumer's surfaces, never registry entries. The concatenation test: for each
surface, the renderer run over a fixture whose every string slot value is the literal `{slot}` (one
row where the renderer loops) equals the surface's groups for that fixture concatenated in registry
order (whitespace-normalized); a slot that is not a string (a number, a count) takes a fixed fixture
value named in the test, and the expected text is the groups filled through `fill` with that value's
rendering, then concatenated; a surface with mutually exclusive branches (a PASS or a FAIL row, an empty
or a joined list, a stage with or without a criterion) gets one fixture per branch, each naming the
groups that branch renders, so every group is rendered by some fixture. The `PLAN_PROTOCOL` and DAG
prompt fixtures run both cap branches: the declared branch's expected text includes `UNIT_SIZE_CAP`
filled with the fixture number the test names, and the DAG prompt's also names `DAG_SIZE_CAP_CAUSE`
in the cause contract's `{size_cap_cause}` slot; the key-absent branch is the second fixture (no cap
sentence, no trailing space, the slot empty). Every group a CLASS TABLE row names as filling another
group's slot enters the expected text the same way, through `fill` into that slot, never appended,
and THE GRANULARITY RULE's equality is read after those fills; a slot the renderer fills from a value
no fixture sets (a constant such as `spawn::CAUSE_SPEC_AMBIGUITY`, or a literal argument such as the
elision note's `{noun}`) enters the expected text through `fill` with that value. For `tool_list` the test
collects every `description` string of the advertised JSON in document order and compares that
list's concatenation with the dash crate's groups. For `rigger prime`'s output the test runs the hook's form (no spec argument; the spec-argument
branch's `spec_lint_next_step` line is operator output outside the registry) and collects the
fixed texts in render order over a fixture store, the store data (decision ids and summaries, the
operator file count) replaced by `{slot}` values. The binary crate's `prompt_surface` module lives in its LIB target (`src/lib.rs` gains `pub mod
prompt_surface`), which `src/cli` reads; it also exposes `pub fn all_groups() -> Vec<&'static Group>`,
collecting every crate's `prompt_groups()` plus its own, the one enumeration.
`every_shipped_agent_text()` (`src/cli/setup.rs:3988`, a bin-test helper) reads `all_groups()`, labelling each group `REGISTRY
<crate>::<group>`; the sweep (`tests/prompt_surface_sweep.rs`) and criterion 17's tests read
`all_groups()` directly, never the bin helper. Each crate's `prompt_surface` module is ungated pure
consts within its crate; `all_groups()` carries the lib's existing store-gate predicate
(`#[cfg(any(feature = "store", not(feature = "core")))]`, `src/lib.rs:19`) for the outer crates'
groups, so the `core` lane compiles the domain's and the binary crate's modules alone, and the sweep,
the concatenation tests and criterion 17's test run on every lane the store predicate admits (default
and no-default), never `core`, gated as the lib's outer modules are.

**THE CLASS TABLE, decided here (criterion 16).** Every surface's groups with the class each group's
doc comment opens with and the const each group's `text` is. THE GRANULARITY RULE: one const per
contiguous class run and per conditionally printed sentence or clause (one behind an `if`, an
`Option`, a non-empty check, a loop or a key being set), every print condition named in the row, so
the registry-order concatenation equals the render on every branch and each branch fixture names the
groups it prints. A slot skeleton carrying no words of its own (`- {id}: {summary}`, `- {p}`) is not a
group, nor is a joiner between filled list items (`later_attempt_block`'s `; and `, `conductor.rs:14737`). Every row below follows its renderer at the base by that rule.
`PLAN_PROTOCOL` (`conductor.rs:13202`) is not reordered: its KEPT sentences render as at the base, in
the base's order, one const per contiguous class run, so the concatenation of its eight runs equals the
render by construction. A const shared by several groups carries ONE class and appears once, with every group that
references it in its row. Each crate's registry test reads its own `prompt_surface` module's source
(`include_str!` of the module file, the same source the sweep reads) and asserts (a) every `pub const`
in the module carries a doc comment whose first line opens with `CONTRACT:` or `FRAMING:`, and (b) the
set of (const name, class) pairs the source yields equals a list pinned in the test, which mirrors
this table row for row, so a class change is a deliberate pin edit reviewed against the table. The
dash crate's list is not an enumeration: its test applies the naming rule over `tool_list`'s advertised
descriptions and property descriptions, and asserts every advertised text has exactly one const named
by the rule and every such const is advertised, each CONTRACT.
Criterion 16 owns the test and the Const column.

| Crate | Surface | Groups (print condition) | Const | Class |
| --- | --- | --- | --- | --- |
| conductor | `EMIT_PROTOCOL` (`build_prompt_with_failure`, `conductor.rs:11687`) | the `rigger_emit` call and its JSON shape (always) | `EMIT_PROTOCOL` | CONTRACT |
| conductor | `EMIT_PROTOCOL` | the sentence "This writes it to the shared event log live, so other agents see it immediately." (always) | `EMIT_LIVE` | FRAMING |
| conductor | `PLAN_PROTOCOL` (a producer stage only, `plan_protocol`) | run 1: the planner's role and baseline sentence and the refine sentence and one space, followed by the `{unit_size_cap}` slot (filled from `UNIT_SIZE_CAP` and a space when `max_unit_added_lines` is declared, else the empty string) | `PLAN_ROLE` | FRAMING |
| conductor | `PLAN_PROTOCOL` | run 2: the split's OWNS sentence, the stable-id sentence and the supersede-and-echo sentence (always) | `PLAN_SPLIT_ID_ECHO` | CONTRACT |
| conductor | `PLAN_PROTOCOL` | run 3: the pairing and extra-unit sentences (always) | `PLAN_PAIRING` | FRAMING |
| conductor | `PLAN_PROTOCOL` | run 4: the copy-the-text sentence alone (always) | `PLAN_COPY_TEXT` | CONTRACT |
| conductor | `PLAN_PROTOCOL` | run 5: the real-split sentence (always) | `PLAN_REAL_SPLIT` | FRAMING |
| conductor | `PLAN_PROTOCOL` | run 6: the `UnitProposed` call and its JSON shape (always) | `PLAN_UNIT_PROPOSED` | CONTRACT |
| conductor | `PLAN_PROTOCOL` | run 7: the unmatched-id and fan-out-gates sentences (always) | `PLAN_UNMATCHED` | FRAMING |
| conductor | `PLAN_PROTOCOL` | run 8: the criteria lead-in with its echo-and-copy parenthetical and `{criteria}` (always) | `PLAN_CRITERIA_LEAD` | CONTRACT |
| conductor | `PLAN_PROTOCOL` | "(no enumerated acceptance criteria)" filling `{criteria}` (only when the spec enumerates no criterion, `conductor.rs:11416-11417`); a criterion's line `- [{id}] {text}` is a skeleton | `PLAN_NO_CRITERIA` | FRAMING |
| conductor | `unit_size_cap`, shared by `PLAN_PROTOCOL` run 1 and the DAG critique prompt | the declared cap sentence (only when `max_unit_added_lines` is declared) | `UNIT_SIZE_CAP` | FRAMING |
| conductor | `UPHELD_PROTOCOL` (the adjudicator prompt, after `REVIEWER_DISCIPLINE`, `conductor.rs:13264`) | the upheld list and its shape, through "a note as well as a blocking one:" (always) | `UPHELD_PROTOCOL` | CONTRACT |
| conductor | `UPHELD_PROTOCOL` | the clause closing that sentence, "the upheld notes are the run's notes, fixed once by the workflow's check-in stage where it has one." (always) | `UPHELD_NOTES` | FRAMING |
| conductor | the check-in stage's run notes (`notes_block`, only when the run upheld a note, `conductor.rs:6115`) | the header and the closing sentence criterion 4 leaves, "Fix every note that is still true; leave a note that is false or already fixed." (:13273, :13283-13285); the line `- {id}{at}: {summary}` is a skeleton | `RUN_NOTES_HEADER`, `RUN_NOTES_CLOSING` | FRAMING |
| conductor | the refused-record note (`RefusedRecord::task`) | the refusal with its `{refusal}` slot (only on the re-run after a refused record, `conductor.rs:1527-1530`); the slot carries the emit tool's recorded refusal, tool output outside the registry | `REFUSED_RECORD_NOTE` | FRAMING |
| conductor | `REQUIRED_PROTOCOL` (the adjudicator prompt, `conductor.rs:8276`) | the verdict line's REQUIRED list and its shape (always) | `REQUIRED_PROTOCOL` | CONTRACT |
| conductor | `REVIEWER_DISCIPLINE`, shared by `review_protocol` and the adjudicator prompt | what the harness enforces or provides (the two worktree sentences; the build-cache sentence is `REVIEWER_BUILD_CACHE`) (always) | `REVIEWER_DISCIPLINE` | FRAMING |
| conductor | `REVIEWER_BUILD_CACHE`, shared by `review_protocol` and the adjudicator prompt | the build-cache sentence (only when the spawn carries a cache variable, criterion 11) | `REVIEWER_BUILD_CACHE` | FRAMING |
| conductor | `review_protocol` | the `rigger_emit` finding call, its JSON shape and the `by` sentence (always) | `REVIEW_FINDING_PROTOCOL` | CONTRACT |
| conductor | `review_protocol` | the sentence "This writes the finding to the shared context graph live, ... see it immediately (via grounding and rigger_peers)." (always; " and address or refute it" cut by criterion 4) | `REVIEW_FINDING_LIVE` | FRAMING |
| conductor | the grounding sections (`graph_context`, only with a seeded subgraph) | the code-neighborhood header, "Code neighborhood of these files (definitions the run extracted into the unified graph):" (only when the section has a definition) | `CODE_NEIGHBORHOOD_HEADER` | FRAMING |
| conductor | the grounding sections | the design-intent header, ending "and the local rationale - reached by the one graph traversal):" (only when the section has a node) | `DESIGN_INTENT_HEADER` | FRAMING |
| conductor | the grounding sections, the full slice | the decisions, lessons and findings headers, "Decisions that govern these files:", "Lessons already learned about these files:" and "Findings other reviewers have already raised about these files:" (each only when its section has a node) | `DECISIONS_HEADER`, `LESSONS_HEADER`, `FINDINGS_HEADER` | FRAMING |
| conductor | the grounding sections | the decisions, lessons and findings elision note with its `{elided}`, `{noun}` and `{recover}` slots (each only when its section elides an entry, `conductor.rs:13797`) | `ELISION_NOTE_RECORDS` | FRAMING |
| conductor | `RECORD_RECOVERY`, shared by that note, `rigger peers` and `rigger graph --around`'s governance tail | the recovery command filling that slot (with that note, `conductor.rs:13537`) | `RECORD_RECOVERY` | CONTRACT |
| conductor | the grounding sections | the definitions elision note and the design-intent elision note, each with its `{recover}` slot (each only when its section elides an entry) | `ELISION_NOTE_DEFINITIONS`, `ELISION_NOTE_DESIGN_INTENT` | FRAMING |
| conductor | `graph_around_recovery`, filling those two slots | the recovery command: no seed, one seed, or several seeds (one branch per seed count, `conductor.rs:13840-13847`) | `GRAPH_AROUND_ANY`, `GRAPH_AROUND_ONE`, `GRAPH_AROUND_EACH` | CONTRACT |
| conductor | `task_block` | the unit's criterion (only for a non-producer stage with a criterion) | `TASK_BLOCK` | FRAMING |
| conductor | `PriorFailure::block` | halted: the captured-commit sentence and "Finish and report; do not start over." (only when `halted_commit` is set, `conductor.rs:1385-1395`) | `PRIOR_HALTED` | FRAMING |
| conductor | `PriorFailure::block` | header: "Your previous attempt failed the checks below. Fix exactly these - do not start over:" (only when a gate, review, REQUIRED or contradiction cause is set, :1396-1404) | `PRIOR_RETRY_HEADER` | FRAMING |
| conductor | `PriorFailure::block` | gate: one line per failed gate's evidence (:1405-1409) | `PRIOR_GATE` | FRAMING |
| conductor | `PriorFailure::block` | review: the review-rejection line (only when `review_reason` is set, :1410-1414) | `PRIOR_REVIEW` | FRAMING |
| conductor | `PriorFailure::block` | REQUIRED lead-in (only when the REQUIRED list is non-empty, :1415-1417) | `PRIOR_REQUIRED_LEAD` | FRAMING |
| conductor | `PriorFailure::block` | PATTERN: the line naming the record id (only for a REQUIRED item with a pattern shape, :1418-1423) | `PATTERN_LABEL`, `PRIOR_PATTERN` | CONTRACT |
| conductor | `PriorFailure::block` | contradiction: the reverted-change sentence and its lead into the contradiction (only when `contradiction` is set, :1425-1435) | `PRIOR_CONTRADICTION` | FRAMING |
| conductor | `ReviewRound::block` (a later review round only) | round lead-in with `{base}` and `{delta}` (always; `{delta}` takes `DELTA_NONE` when the delta is empty, :13313-13318, :13333-13335) | `ROUND_LEAD_IN` | FRAMING |
| conductor | `ReviewRound::block`, `later_attempt_block` | the empty-delta fill "nothing" (only when the delta is empty) | `DELTA_NONE` | FRAMING |
| conductor | `ReviewRound::block` | the REQUIRED header (only when the round required an item, :13324) or the none-line "That round required no item." (only when it required none, :13320) | `ROUND_REQUIRED_HEADER`, `ROUND_REQUIRED_NONE` | FRAMING |
| conductor | `ReviewRound::block` | PATTERN: the line naming the implementer's audit record (only for a REQUIRED item with a pattern shape, :13325-13328) | `PATTERN_LABEL`, `ROUND_PATTERN` | CONTRACT |
| conductor | `ReviewRound::block` | the outside-delta rule and the `required` sentence (always, :13336-13338) | `ROUND_OUTSIDE_DELTA` | CONTRACT |
| conductor | `gate_evidence_block` (only with a commit and a gate run on it) | its framing sentence (always) | `GATE_EVIDENCE_HEADER` | FRAMING |
| conductor | `gate_evidence_block` | one row per gate with its verdict and digest, its evidence lines indented below it (:617-623) | `GATE_EVIDENCE_ROW` | FRAMING |
| conductor | `conflict_resolution_prompt` | the conflict facts and the resolve-and-commit sentences (always) | `CONFLICT_RESOLUTION` | FRAMING |
| conductor | `later_attempt_block` (only when it has a cause, :14731-14738) | the LATER ATTEMPT frame with its `{causes}` slot (always) | `LATER_ATTEMPT` | FRAMING |
| conductor | `later_attempt_block` | the review cause (only when a delta is given, :14718-14727) and one gate cause per failed gate (:14728-14730), joined by "; and " | `LATER_ATTEMPT_REVIEW_CAUSE`, `LATER_ATTEMPT_GATE_CAUSE` | FRAMING |
| conductor | `DOC_ONLY_ROUND` | the fact (only on a round whose delta changed documentation only, `conductor.rs:5562`, :5569) | `DOC_ONLY_ROUND` | FRAMING |
| conductor | `write_peers_pointer` | the retrieval facts (only on the implement slice, in place of the capped sections) | `PEERS_POINTER` | FRAMING |
| conductor | `write_lookup_pointer` | the lead-in "The knowledge graph answers lookups through three verbs:" (always, with a seeded subgraph) | `LOOKUP_LEAD_IN` | FRAMING |
| conductor | `write_lookup_pointer` | the three verbs and their jobs (always, with a seeded subgraph) | `LOOKUP_VERBS` | CONTRACT |
| conductor | the DAG critique prompt | the prior-reject block with its `{prior_reason}` slot (only when a prior critique rejected, `conductor.rs:8375-8379`) | `DAG_PRIOR_REJECT` | FRAMING |
| conductor | the DAG critique prompt | the opener (always) | `DAG_CRITIQUE_OPENER` | FRAMING |
| conductor | the DAG critique prompt | the units header and one line per proposed unit, `DAG_NO_CRITERION`, `DAG_NO_NEEDS` and `DAG_NO_RADIUS` filling an empty criterion, needs list or radius (:8387-8408); `DAG_UNIT_ROW` spells `criterion="{criterion}"` with the quotes in its text, the renderer filling the slot with the criterion's escaped form (its `{:?}` rendering less the outer quotes), `DAG_NO_CRITERION` included, so the row renders byte-identical to the base | `DAG_UNITS_HEADER`, `DAG_UNIT_ROW`, `DAG_NO_CRITERION`, `DAG_NO_NEEDS`, `DAG_NO_RADIUS` | FRAMING |
| conductor | the DAG critique prompt | the disjoint-overlap line (only with no conflict) or the overlap header and one line per conflicting pair (only with a conflict, :8409-8425) | `DAG_OVERLAP_DISJOINT`, `DAG_OVERLAP_HEADER`, `DAG_OVERLAP_ROW` | FRAMING |
| conductor | the DAG critique prompt | the verdict and cause contracts with the `{spec_ambiguity}` slot (filled from `spawn::CAUSE_SPEC_AMBIGUITY`, `conductor.rs:8441`) and the `{size_cap_cause}` slot (always) | `DAG_CRITIQUE_CONTRACT` | CONTRACT |
| conductor | the DAG critique prompt | "a unit over the size cap, " filling `{size_cap_cause}` (only when `max_unit_added_lines` is declared, else the empty string; criterion 7) | `DAG_SIZE_CAP_CAUSE` | CONTRACT |
| conductor | `re_plan` | the rejection with its `{feedback}` slot and the revise sentence (always) | `REPLAN_LEAD` | FRAMING |
| conductor | `re_plan` | the exact-id refine and new-id sentences (always) | `REPLAN_ID_CONTRACT` | CONTRACT |
| conductor | the canary's `review_header` | the item framing with its `{file}` slot (always; `CANARY_NO_ANCHOR` fills it when the item has no anchor, `canary_store.rs:648-652`) | `CANARY_REVIEW_HEADER`, `CANARY_NO_ANCHOR` | FRAMING |
| conductor | the canary's `adjudicator_prompt` | the findings lead-in (always) and the no-findings line (only with no finding, `canary_store.rs:666-667`) | `CANARY_FINDINGS_LEAD`, `CANARY_NO_FINDINGS` | FRAMING |
| conductor | the canary's `adjudicator_prompt` | the verdict line `verdict_approves` reads (always, :676-677) | `CANARY_VERDICT` | CONTRACT |
| domain | the critique prompt | its framing (always) | `CRITIQUE_FRAMING` | FRAMING |
| domain | `CRITIQUE_OUTPUT_CONTRACT` | the finding lines, the remedy field included (always) | `CRITIQUE_OUTPUT_CONTRACT` | CONTRACT |
| domain | `CRITIQUE_RECORDED_FINDINGS` | the recorded-findings lead-in and the report-only-new contract, its verdict line included (only when the text has recorded findings, `review.rs:348`) | `CRITIQUE_RECORDED_FINDINGS` | CONTRACT |
| domain | the critique prompt | the spec tail "The spec under critique, `{spec_path}`, verbatim:" with `{spec_text}` (always, `review.rs:375`) | `CRITIQUE_SPEC_TAIL` | FRAMING |
| domain | the composed system prompt (`instructions::compose`, every spawn, `instructions.rs:156`) | the built-in layer's heading "# Rigger instructions" (always), which `render` and the prime surface's first group also read | `INSTRUCTIONS_HEADING` | FRAMING |
| driver | `progress_framed` (both process hosts, `crates/rigger-driver/src/driver/mod.rs:63`) | the separator "--- rigger driver ---" after the task (always) | `PROGRESS_FRAME` | FRAMING |
| driver | `progress_framed` | "LIVE PROGRESS: record a progress line {report}." (always) | `PROGRESS_LINE` | CONTRACT |
| driver | each process host's `framed` | the report verb filling `{report}`: the cli host's `rigger progress` command, the headless host's `rigger_progress` tool call (each on its own host) | `CLI_PROGRESS_VERB`, `STREAM_PROGRESS_VERB` | CONTRACT |
| driver | the headless host's `interruption_note` | "This session was interrupted{why}. Continue where you left off." (on every resumed launch) | `RESUME_NOTE` | FRAMING |
| driver | `interruption_note` | the `{why}` fill: the supervisor restart (with no class), the released hold with `{held_for_s}` and `{reason}` (after a hold), or " by {class}" (otherwise), each on its own branch | `RESUME_RESTARTED`, `RESUME_HELD`, `RESUME_CLASS` | FRAMING |
| dash | the MCP tool descriptions and `inputSchema` property descriptions | each tool and argument `tool_list` advertises (always) | the naming rule: a surface-specific text is `<SURFACE>_<TOOL>` for a description and `<SURFACE>_<TOOL>_<ARG>` for a property; a shared text is one const named `<TOOL>` (`<TOOL>_<ARG>` for a shared property) with no surface prefix; the test derives the list by the rule, no enumeration; each text is the advertised meaning of its tool or argument and so CONTRACT whole, its purpose clauses included | CONTRACT |
| binary | `GREP_GUARD_MESSAGE` (`src/cli/mod.rs:5526`) | the guard's rule and the `# --literal` marker it parses (on every held call) | `GREP_GUARD_MESSAGE` | CONTRACT |
| binary | the `rigger guard-write` deny reason (`src/cli/guard.rs:418`) | the first root the write falls outside of (on every denial, the unreadable-target one included) | `GUARD_WRITE_DENY` | FRAMING |
| binary | `rigger guard-write`'s exit-2 stderr texts (`src/cli/guard.rs:225`, :228, :232, :241, :248, after the prefix "rigger guard-write: " at :483) | the `--root` arguments the installed hook was given (each only on its own parse failure; the prefix on every one) | `GUARD_WRITE_STDERR_PREFIX`, `GUARD_WRITE_ROOT_MISSING`, `GUARD_WRITE_ROOT_EMPTY`, `GUARD_WRITE_ROOT_RELATIVE`, `GUARD_WRITE_UNKNOWN_ARG`, `GUARD_WRITE_NO_ROOT` | FRAMING |
| binary | `rigger guard-write`'s exit-2 stderr text (`src/cli/guard.rs:489`) | the payload the hook could not read (only on a stdin read failure) | `GUARD_WRITE_PAYLOAD_UNREADABLE` | FRAMING |
| binary | `rigger prime`'s output (`src/cli/prime.rs`) | the instructions-in-force line (always): the domain's `INSTRUCTIONS_HEADING`, then " in force: {builtin}{extra} - see rigger instructions" with its `{extra}` slot, which `PRIME_OPERATOR_FILES` fills only when an operator file exists (`src/cli/setup.rs:1615-1622`) | `PRIME_IN_FORCE`, `PRIME_OPERATOR_FILES` | FRAMING |
| binary | `rigger prime`'s output | the no-store line (only with no store, :1644) or the decisions header (only with a store, :1659) and the none-yet line (only when no decision shows, :1670-1672) | `PRIME_NO_STORE`, `PRIME_DECISIONS_HEADER`, `PRIME_NONE_YET` | FRAMING |

**THE PHRASE AND MARKERS TESTS, decided here (criterion 17).** One test, in `src/cli/setup.rs`'s
test module, calling the MARKERS function as `crate::test_support::language_marker_hits`,
iterates `rigger::prompt_surface::all_groups()` directly and the scripts' contract and framing region
entries (criterion 18, which 17 needs), read through the same region reader the sweep uses, and fails on any phrase of
a list kept in the test, matched as a lowercase substring: "the moment", "never batch", "do not
batch", "keep it flowing", "between your actions", "between your own actions", "do not contradict",
"supersede explicitly", "default to skepticism", "reject only", "not by grepping", "never a new test",
"audit the whole tree", "do not write code", "scope creep", "for code lookups", "read first", "honor them",
"do not repeat", "address or refute". A second assertion runs the
MARKERS rule (`language_marker_hits`, `tests/common/fixtures/language_markers.rs:9`, called, never
copied) over the same texts,
a script literal's `${...}` interpolations removed first. A negative fixture holds the base's
`EMIT_PROTOCOL` text in the test and asserts the phrase check flags it. The test never reads
`BUILTIN`, the personas or the skills: prescription lives there by design.

**THE SOURCE SWEEP, decided here (criterion 18).** A test walks every `.rs` file under `crates/` and
`src/` that criterion 2's production-boundary reader (`tests/common/fixtures/production_boundary.rs`)
keeps, and reads each through that reader's cut; the rule is the one SPAWN HOST RESOLUTION defines, and
the sweep restates none of it. The PROMPT-BEARING files are derived by token, never
listed by hand: every such file whose production text spells `SpawnOpts`, `SpawnHost` or
`prompt_surface`, so a new file touching the spawn surface or the registry is swept automatically. The token match runs
over code outside comments: a line whose first non-blank characters are `//` is a comment line and
matches nothing. The lib files that declare `pub mod prompt_surface;` (`src/lib.rs` and each
registry crate's `lib.rs`: `rigger-domain`, `rigger-conductor`, `rigger-driver`, `rigger-dash`) are
members of the set holding no qualifying literal (0 each). The token marks every renderer, since each crate's module carries the name `prompt_surface`. The
test tokenizes their string literals (escaped and raw, line continuations joined) and selects every
literal of 40 or more characters containing a space, by shape alone. Each selected literal outside a
crate's `prompt_surface` module is in a registry group (its exact text, or contained in a registered
group) or in an allow-list kept in the test, keyed by file, with two named categories: "operator
message" (an error, a status line, a help text, a log line the operator reads, a hook command line the host runs) and "tool output" (a
tool result or error an agent reads back, such as the MCP tool results and errors in `crates/rigger-dash/src/mcpserver.rs`),
one prefix entry per literal. A hook's denial reason or exit-2 stderr text is a fixed template the
binary puts before an agent through the hook channel, so it is registry text (THE CLASS TABLE's hook
rows), never tool output: that category is for text a tool returns as data (search results, file
contents, a build's own errors), never a hook template the binary authored (criterion 18). The sweep also selects every qualifying literal INSIDE a crate's
`prompt_surface` module and requires each to equal, or be contained in, the `text` of some group
`all_groups()` returns; a module literal no group carries fails the gate the same way an unregistered
literal outside the module does (criterion 18). Tool results and errors are OUTPUT, data an agent reads back, not text
placed before it as instruction, so they are outside the prompt surface and the registry; criterion
18 owns every entry of both categories, among them `crates/rigger-driver/src/hooks.rs`'s two
qualifying literals (the StopFailure hook command line at :92, a command the host runs that no agent
reads, and the helper frontmatter error at :127), `src/cli/critique.rs`'s nine (:57, :101, :196, :232,
:236, :239, :246, :305, :355), `src/cli/guard.rs`'s eight `rigger hook stop-failure` texts (:509, :513, :540, :547, :553,
:563, :574, :598: `cmd_hook` returns its errors through the generic exit-1 path and the StopFailure
event fires after the session has stopped, so only an operator reads them), `spec_lint_next_step` (`src/cli/mod.rs:5428`,
which stays there as operator output) and `crates/rigger-domain/src/instructions.rs`'s one
(`shadow_advisories`' warning at :211, which `rigger validate` prints) as operator messages. `GREP_GUARD_MATCHER` ("Grep|Bash", 9
characters) and `GREP_GUARD_COMMAND` ("rigger grep-guard", 17) in `crates/rigger-driver/src/hooks.rs`
are hook configuration
values, not prompt text; they stay where they are and, under 40 characters, need no entry. `setup.rs` and `docs.rs` spell none of the three tokens and no
criterion makes them do so, so the scaffold seeds and the skill and handbook bodies are outside the
sweep and need no allow-list entry. A literal of 40 or more
characters with a space in a prompt-bearing file that is neither registered as contract or framing
nor allow-listed as an operator message therefore fails the test. The residue, stated and accepted: a
literal in a file spelling none of the three tokens that reaches a prompt through a function call is
outside the sweep; criterion 16's concatenation test closes it for every surface that test covers.
For the two embedded scripts (`RIGGER_WORKFLOW`, `SHIM_MJS`), the script itself stays the text's one
home: each literal of 40 or more characters with a space (interpolations removed) sits inside a region
opened by a `// prompt-surface: contract`, `// prompt-surface: framing` or `// prompt-surface:
operator` comment line and closed by `// prompt-surface: end`, and the test fails on such a literal
outside a region. An operator region holds text no agent reads (log lines, phase labels, error
strings) and is excluded from criterion 17's assertions. Each contract and framing region becomes ONE entry, built by a region reader in the lib's
`prompt_surface` module from the script text, and also added to `every_shipped_agent_text()`: all of its
literals, of any length, joined in source order (`+`-concatenated literals with no separator, all
others with a single space, each whitespace run of the entry then collapsed to one space),
interpolations written `{}`
(a template literal nested inside an interpolation is a literal of its own, joined into the entry in
source order and selected by the 40-character rule),
labelled `SCRIPT <file>:<line>` by the region's opening line; criterion 17's two assertions run over
that joined entry. The shim's `buildProxyServer` tool and argument descriptions
(`shim/shim.mjs:163-207`) sit in `// prompt-surface: contract` regions, one per tool, each closing
before the tool's handler (whose result text is under 40 characters), CONTRACT whole as the dash
row's descriptions are. `meta.description` (`rigger.js:52`), which the host lists to a model as the
workflow's description, sits in a `// prompt-surface: contract` region, CONTRACT whole as a tool
description; the phase details (:54-57) and the display-label tables `PERSONA_VERB` and `ROSTER_VERB`
(:386-409, which `workerLabel` renders into a spawn's display label) sit in operator regions.

**OPERATOR HAND-OFF, decided here.** No unit edits `.rigger/workflow.yml`: the run's driver is the
installed binary, whose `Defaults` refuses an unknown key (`deny_unknown_fields`, `crates/rigger-domain/src/config.rs:747`),
so a unit landing a new key would halt every later step. After the run's PR merges, the operator
refreshes the binary, then adds `grep_guard: true`, `max_unit_added_lines: 3000` and `test_author:
sdet-author` under this repository's `defaults`, then runs `rigger setup` (which with the key
declared keeps the guard). `.rigger/docs-overlay.yml` (criterion 13) binds the same way: a pre-121
binary's `DocsOverlay` refuses the unknown `global_constraints` field (`deny_unknown_fields`), so no
`rigger setup` runs with a pre-121 binary from criterion 13's integration on `rigger-run` until the
merge and the refresh; the refresh comes first. A `rigger setup` refusal on `global_constraints` from the installed pre-121
binary inside a unit worktree (criterion 14 onward carries `.rigger/docs-overlay.yml`) is expected
until the refresh and is no finding; a unit rehearses setup only with the tree's own binary by
explicit path. Persona edits (criteria 6, 7, 8) change files `definition_hash` folds
(`src/cli/mod.rs:1142`); each such unit's integration is a definition drift the operator resumes
with `--rebase-definition`. This repository tracks its helper copies
(`.claude/agents/lookup.md`, `verify.md`), so their fold changes its definition hash: its paused run
relaunches once with `--rebase-definition` after the key edit and the `rigger setup` that follows it
(the order: refresh, then the key edit, then `rigger setup`, and only then the relaunch).

**CONSTRAINTS WALK, decided here.**

| Mechanism | Corner | Decision |
| --- | --- | --- |
| new workflow keys | empty | absent: no guard, no cap sentence, no test-author spawn |
| new workflow keys | existing data | an existing workflow.yml without them loads and behaves as declared, not as before: an upgraded project loses its guard (setup removes rigger's entry), its cap and its sdet-author spawn until it declares them; accepted and named in the handbook's upgrade note (criterion 2's, 7's and 10's lines in `authoring-loops.md`) |
| new workflow keys | an older binary reading a new workflow | refused at load (`deny_unknown_fields`); OPERATOR HAND-OFF orders the refresh first |
| new workflow keys | reverted, DROPPED | a key removed between steps is a definition drift, as any workflow edit; rebased, the next spawn reads the new value |
| new workflow keys | concurrent, crash-resume, cold start | read from the one loaded `Config` per process, never cached across processes |
| `test_author` | repeated | one key, one agent; the role token stays `sdet-author`, so a renamed agent resumes the token's recorded session, as a renamed adjudicator does at the base; accepted |
| `test_author` | existing store | recorded ids unchanged; a store whose run spawned the conventional agent replays by id |
| `grep_guard` | existing data | a settings.json carrying the guard and a foreign PreToolUse hook, in one block or two: setup removes only inner hooks whose command is `GREP_GUARD_COMMAND`, any matcher, dropping a block only once its inner array is empty |
| `grep_guard` | repeated | setup twice with the key declared writes once; absent twice removes once |
| `grep_guard` | empty | no `settings.json` and the key off: nothing read or written, no `.claude/` directory created, `AlreadyCurrent`; driven on `install_lookup_hook` alone, since `rigger setup` writes `settings.json` in `init_project` first |
| `grep_guard` | existing data, key absent | an upgraded project's first `rigger setup` removes the installed guard and prints the removal line, never an install line |
| `grep_guard` | drifted matcher | an installed guard hook whose matcher differs from `GREP_GUARD_MATCHER`: an install with the key true replaces it, so exactly one entry with the current matcher remains |
| `grep_guard` | reverted | the key flipped false after a setup: the installed hook denies until the next `rigger setup`; accepted |
| spawn host | existing data, replay | `cmd_replay` resolves once, inside `materialize_config_at_rev` before the checkout is removed: the candidate config's defaults decide the flag, the `--against` checkout root the helpers |
| spawn host | concurrent, crash-resume, cold start | `resolve_spawn_host` reads the loaded defaults and the root once per step, critic run or canary run; every process resolves again |
| helper copy | empty | no `.claude/agents/` file: the embedded text |
| helper copy | DROPPED | an operator copy deleted from the operator's checkout mid-run: the copy present at the pin folded into `definition_hash` and is gone, so the next step halts on drift; resumed with `--rebase-definition`, the next spawn reads the embedded text, whatever a unit worktree still carries |
| helper copy | existing pinned run | a run pinned before the refresh in a project with a copy present resumes once with `--rebase-definition`; without a copy it resumes unchanged |
| helper copy | run start, replay | the fresh-run fingerprint folds the operator checkout's present copies; the replay folds and spawns with the `--against` checkout root's (its tracked `.claude/agents/` when present, the embedded text otherwise), one value for `definition_hash` and `Deps::spawn_host`; no caller reads them another way |
| helper copy | edited mid-run | under `rigger step`, a definition drift, halted and resumed with `--rebase-definition`, as a persona edit is; `run_cli` and `run_workflow` keep the value they resolved at start for the whole process, so a mid-run edit under them takes effect at the next launch |
| helper copy | existing data | an operator copy written by an older `rigger init` and never edited equals the embedded text at that version; a newer binary keeps passing the old copy; accepted (the copy is the definition) |
| helper copy | malformed | `resolve_spawn_host` returns `Err` naming the file once, where its caller resolves the spawn host, before any spawn and before the pin check, charging no attempt; `rigger validate` names it first |
| skill shadow | existing data | a hand-edited installed skill with no project copy is rewritten as at the base |
| skill shadow | near name, repeated | a near name shadows nothing; a second setup writes nothing |
| skill shadow | DROPPED | a project copy deleted: the next setup installs the shipped render |
| overlay field | empty | absent or empty list: the generic step only |
| scaffold implementer | existing data | an existing project's workflow and personas are write-once and stay; only a fresh `rigger init` gets the new ids |
| gate cache | degraded guard, deferred and regenerate | the regenerate `run_regenerate_command` passes `ProbeFault` up through integration to `settle_stage`'s `HALT_MARKERS` arm; the deferred `run_deferred_gates` strips the marker through the shared fold and propagates it through its `?` at `conductor.rs:2684`, recording no deferred verdict and no `DeferredGateFailed`; both end `conductor::run` with the halt `Err` (criterion 19) |
| spawn cache | no unit sibling | `spawn_env` adds no cache variable: the planner at an empty dir builds in the operator checkout's own build directory, a reviewer in a review-only worktree builds inside that worktree, which its teardown removes; no new cache class (criterion 11) |
| spawn cache | concurrent | a reviewer and a gate building different trees never share a directory, by construction: the gate builds in the shared gate cache or the unit's cache, the reviewer in its review cache sibling or its own worktree (criterion 11) |
| spawn cache | concurrent review-only-worktree builds | ACCEPTED: two homes, each with its reaper. A no-sibling reviewer's reproduction builds in its throwaway scratch worktree under its registered scratch path (SCRATCH POLICY), reaped once its result records; a build it runs in place in the review-only worktree lands in that worktree's toolchain-default directory (the lenses of one review attempt share it), torn down with the worktree by the existing teardown; the sibling-cache alternative is declined because it adds a cache class with its own reaper and liveness rule. The planner's builds land in the operator checkout's own build directory beside the operator's builds, accepted (criterion 11) |
| gate cache | degraded guard | an infrastructure fault (criterion 19), a `HALT_MARKERS` halt `run_gates_at` propagates before any `GateOutcome`, bypassing the F3 rerun: the step ends with the halt `Err` naming the probe error, no attempt charged, no gate verdict or `PriorFailure` written, the pass's earlier verdicts kept and replayed on resume; the faulted unit's worktree and cache are torn down and its branch kept, as for every `HALT_MARKERS` halt; nothing builds into the gated tree, the shared cache or the ambient directory |
| gate cache | concurrent: two units in one wave, one faults | `settle_stage` keeps the halt as the wave's first error; `run_wave` (`conductor.rs:4264`) lets every other running or still-admissible unit run to its own end or park, as at the base for any `HALT_MARKERS` halt, keeping its verdicts, then returns that `Err` (criterion 19) |
| gate cache | cold start, repeated | the probe creates a missing guard parent first, so a cold scratch root is usable; a real fault ends the step with the halt `Err` naming the guard path, the OS error and the operator step, and the relaunched step probes again (criterion 19) |
| gate cache | no guard, no directory | a gate run that requests no guard and names no directory inherits the ambient environment; no production caller does this; accepted residue |
| `build_cache_env` | empty | never null: a wave item with no per-unit cache still carries the name, so SCRATCH POLICY's export sentence renders for every spawn, as at the base |
| scope sentence | DROPPED | `unit_size_cap`'s second sentence leaves the DAG prompt and lands in the three critique and planning personas and their scaffold templates in the same commit |
| scope sentence | `max_unit_added_lines` absent | the moved scope sentence stays in the three personas and is inert: it qualifies a cap only when the prompt states one; accepted |
| moved sentences | existing consumer personas | ACCEPTED: write-once personas are never rewritten; the handbook upgrade note names each moved sentence and its persona home |
| registry | empty surface | a renderer whose optional parts are all absent renders a subset of its groups; the concatenation fixture covers every branch present |
| sweep | a new literal | fails until registered or allow-listed |
| sweep | an orphan module const | a `prompt_surface` module literal of 40 or more characters with a space that no group carries fails the sweep (criterion 18); a shorter const a renderer reads but no group references fails criterion 16's concatenation test, since the branch fixture that renders it cannot match the groups' concatenation |
| moved sentences | switched off | a consumer switching `working-discipline` or `communication` off removes the moved sentences with the rest: intended |
| moved sentences | MARKERS | each moved sentence carries no MARKERS hit, so the MARKERS test over the shipped texts stays green |

By a later operator decision on graph adoption, the grep guard's denial names the graph call, and
the graph tool description and the lookup pointer each carry one worked example; criterion 2's
exact-message pin and the method cut hold everywhere except those three texts.

The in-session workflow path (the workflow driver, `rigger serve` and its workflow-only MCP tool surface) and the `rigger step` courier commands have been removed, so a criterion naming one of those surfaces is met by its removal, and a criterion naming a step holds for `rigger run`, the one driver.

## Notes (non-criteria)

INTERIM SENTENCES: a doc or handbook sentence a later criterion makes false is left by an earlier
unit as written and raises no finding. Assignments: `write_lookup_pointer`'s
and `write_peers_pointer`'s docs to 1; `GREP_GUARD_MATCHER`'s doc, `install_session_settings`'s doc,
`spawn_config_args`' doc bullet on `--settings`, `tools-and-context.md:109` and the using-rigger sentence
(`docs.rs:190-198`) to 2; `HELPER_AGENTS`'
doc ("cannot drift from them"), the `--agents` bullet of `spawn_config_args`' doc and
`tools-and-context.md:111` to 3; `EMIT_PROTOCOL`'s and `DECISIONS_HEADER`'s docs to 4; the shim's and
the script's `progressNote` comments and the docs of `progress_framed` and both `framed`s to 5; `DOC_ONLY_ROUND`'s doc to 6;
`MAX_UNIT_ADDED_LINES`' doc (deleted) and `unit_size_cap`'s doc to 7; `PLAN_PROTOCOL`'s doc ("a
proposed unit that maps to NO criterion is scope creep and is refused") to 8; `ReviewRound`'s `audit`
field doc to 9; `spawn_sdet_author`'s doc to 10; the `buildLocation` comment (`rigger.js:679-683`) to
11; the `SCAFFOLD_AGENTS` and `SCAFFOLD_WORKFLOW` docs and `authoring-loops.md:186` to 15; the
handbook's upgrade note for the three keys (a new paragraph after `authoring-loops.md`'s "Upgrading an
older workflow" paragraph, one sentence per key) to 2, 7 and 10 in that order, each adding its own
sentence; the same note's persona lines (one per moving criterion, naming each sentence it moves and
that sentence's persona home, since an existing project's write-once personas never gain it) to 6,
7 and 8, each adding its own line; each item of the open-exceptions list in `authoring-loops.md`'s `## What rigger prescribes`
to the criterion EXCEPTIONS LIST names (1, 4, 5, 6, 7, 8, 9, 11, 12), and the emptied list with its
lead-in sentence to 17.

OUT OF SCOPE: the build-cache variable's value (the constant itself is criterion 11's) and the
grounder's Rust-only extraction; the SessionStart hook mechanism (its `rigger prime` output's fixed texts are criterion 16's registry
surfaces); the status line (`console::statusline`, `rigger status --line`), the editor's display,
read by the operator and never by a spawn's model, so outside the Goal's claim;
the liveness heartbeat, the self-report and the courier relay contracts; every other shipped skill's
body (each becomes shadowable by criterion 14); `HELPER_AGENTS`' text; the personas' other content;
the `mutation.sh` template.

LANES (criterion 20): the `core` lane compiles each ungated `prompt_surface` module of the domain and
the binary crate alone; `all_groups()`, the sweep, the concatenation tests and criterion 17's test run on
every lane the store predicate admits (default and no-default), never `core` (THE REGISTRY).

UNIT ORDER (the needs chain): 1, 2, 3, 4, 5, 6, 8, 7, 9, 10, 19, 11, 12, 13, 14, 15, then 16, then 18,
then 17, then 20. The moves 1 to 15 and 19 are independent except three named needs: 3 needs 2, 7
needs 8, and 11 needs 19 (shared files otherwise serialize through the blast radius). 16 needs 1 to 15
and 19; 18 needs 16; 17 needs 18; 20 needs all.

LANDING-ORDER SIMULATION (every ordered pair sharing a surface):

- 1, 4 and 9 (the `working-discipline` body): 1 adds sentences to the Fan out section, 4 a paragraph to
  `## One pass, by excellence` and two sentences to `## When you review`, 9 adds none and pins the
  hand-over section's existing bullet -> yes in any order.
- 4 and 11 (`review_protocol`, `tests/critique_method_persona_periphery.rs`'s
  `ADVERSARY_FINDINGS_PROTOCOL`): 4 cuts two clauses of the findings sentences, 11 moves the
  build-cache sentence out of `REVIEWER_DISCIPLINE` into the conditional `REVIEWER_BUILD_CACHE` and out of
  `ADVERSARY_FINDINGS_PROTOCOL`, whose plan-critique adversary carries no cache, each unit editing only
  its own clauses of that const -> yes either order (the blast radius serializes them).
- 4 and 5 (`shim.mjs`): 4 edits `renderPeers`, 5 the progress note -> yes either order.
- 4 and 8 (`PLAN_PROTOCOL`): the planner's cadence clause is 8's alone -> yes.
- 8 then 7 (`PLAN_PROTOCOL`, the planner personas): 8 moves its sentences and rewords the two MARKERS
  hits; 7 then renders the cap slot and places its moved sentence directly after 8's split sentence
  -> yes. 7 first: the split sentence it follows does not exist -> fixed by order (7 needs 8).
- 2 then 3 (`SpawnHost`, `SpawnOpts`, `Deps::spawn_host`, `spawn_config_args`): 2 lands `SpawnHost`
  with `grep_guard`, `resolve_spawn_host(defaults)`, `Deps::spawn_host`, the accessor, the six
  resolution points' guard flag, every construction and `spawn_config_args`' host parameter; 3 adds the `repo_root` argument, `helpers`, its fold and
  its readers -> yes. 3
  first: no `SpawnHost` to extend -> fixed by order (3 needs 2).
- 6 and 10 (`spawn_sdet_author`): 6 first: its test spawns by the convention; 10 then adds
  `test_author` to that fixture, which it owns -> yes. 10 first: 6's test declares the key -> yes.
- 5 and 11 (`rigger.js`): 5 the progress note, 11 the other worker and courier texts -> yes.
- 19 then 11 (`ExecRunner::run`, the courier): 19 makes a failed probe halt the step; 11 then drops
  the courier prefix -> yes. 11 first: a failed probe still falls back to the ambient variable the
  prefix no longer fills -> fixed by order (11 needs 19).
- 13 and 14 (`install_skills`): 13's installed-copy test needs only the overlay; 14 installs a shadow
  verbatim, never overlaid -> yes either order.
- 2, 7, 15 (`SCAFFOLD_WORKFLOW`): each adds its own key line or placeholder -> yes.
- 16 after 1 to 15 and 19: 16 relocates final texts; before them it would relocate texts later criteria
  edit -> fixed by order.
- 18 without 16: every selected literal would be unregistered -> fixed by order (18 needs 16).
- 17 without 18: no script regions to iterate -> fixed by order (17 needs 18, which needs 16, which
  needs the moves, so no phrase hit remains).

TEST PLACEMENT:

| Criterion | Test homes |
| --- | --- |
| 1 | `conductor.rs` tests (the pointer's text); `src/cli/setup.rs` tests through the shipped-text helper (the built-in's section) |
| 2 | `tests/common/fixtures/production_boundary.rs` (the shared reader, reached as `crate::test_support::...` from `src/cli/setup.rs`'s test module, where the production-line pin and the no-`SpawnHost {` assertion call it); `crates/rigger-driver/src/hooks.rs` tests (merge with and without the flag); `src/cli/setup.rs` tests (`install_lookup_hook` install, remove, foreign hook kept, a drifted matcher replaced, no `.claude/` created with the key off); `crates/rigger-config-files/src/config_store.rs` (the key parses); `src/cli/mod.rs` tests (`GREP_GUARD_MESSAGE` equals the decided text) |
| 3 | `crates/rigger-driver/src/hooks.rs` tests over a temp root with and without an operator copy, a malformed copy failing `resolve_spawn_host`; `crates/rigger-driver/src/driver/mod.rs` tests (`--agents` and the pre-approval from one copy); `src/cli/mod.rs` tests (`definition_hash` changes with an operator copy) |
| 4 | `conductor.rs` tests (`EMIT_PROTOCOL`, `review_protocol`, the grounding headers, `notes_block`); `src/cli/mod.rs` tests over `SHIM_MJS`; `src/cli/setup.rs` tests through the shipped-text helper (the two `working-discipline` landings); `tests/critique_method_persona_periphery.rs` (`ADVERSARY_FINDINGS_PROTOCOL`) |
| 5 | `crates/rigger-driver/src/driver/mod.rs`, `crates/rigger-driver/src/driver/cli.rs`, the headless host driver module's and `crates/rigger-dash/src/mcpserver.rs` tests; `src/cli/mod.rs` tests over `RIGGER_WORKFLOW` and `SHIM_MJS`; `instructions.rs` tests (the built-in's bullet) |
| 6 | `conductor.rs` tests (the doc-only sdet prompt); `src/cli/setup.rs` tests over the committed persona |
| 7 | `conductor.rs` tests (both prompts with the key declared and absent, the DAG fixtures naming `DAG_SIZE_CAP_CAUSE`); `config_store.rs` (the key); `src/cli/setup.rs` tests (personas, seeds, `SCAFFOLD_WORKFLOW`) |
| 8 | `conductor.rs` tests (`PLAN_PROTOCOL`); `src/cli/setup.rs` tests (persona and seed, each moved sentence asserted on its own, never the block's contiguity, so criterion 7's later insertion keeps them green) |
| 9 | `conductor.rs` tests (both PATTERN lines); `src/cli/setup.rs` tests (the bounded-audit bullet stands) |
| 10 | `conductor.rs` tests (declared, absent with `sdet-author` defined, an undefined id refused); `config_store.rs` tests (the refusal) |
| 11 | `spawn.rs` tests (the `build_cache_env` wire field); `src/cli/mod.rs` tests over `RIGGER_WORKFLOW`; `conductor.rs` tests (a spawn in a review-only worktree and the planner at an empty dir carry no cache variable; a unit spawn carries its sibling through `spawn::BUILD_CACHE_ENV`; the two `review_protocol` fixtures: with a cache variable it renders `REVIEWER_DISCIPLINE` then `REVIEWER_BUILD_CACHE`, without one `REVIEWER_DISCIPLINE` alone); `tests/critique_method_persona_periphery.rs` (`ADVERSARY_FINDINGS_PROTOCOL` without the build-cache sentence) |
| 12 | `crates/rigger-dash/src/mcpserver.rs` tests |
| 13 | `docs.rs` tests (the generic step); `src/cli/mod.rs` tests (`install_skills` with an overlay list) |
| 14 | `src/cli/mod.rs` tests over a temp root (a shadow, a near name, a deleted shadow); a periphery test running the built binary's `setup` |
| 15 | `src/cli/setup.rs` tests (`init_project` with and without `Cargo.toml`) |
| 16 | each crate's `prompt_surface` tests (no duplicate (surface, name); the per-crate registry test reading its module's source by `include_str!`: every `pub const` opens its doc with `CONTRACT:` or `FRAMING:`, and the (const, class) pairs equal the pinned list); a lib test that `all_groups()` holds every crate's `prompt_groups()`; a concatenation test beside each renderer (`conductor.rs`, `review.rs`, `canary_store.rs`, `crates/rigger-driver/src/driver/mod.rs`, the headless host driver module, `crates/rigger-dash/src/mcpserver.rs`, `src/cli/mod.rs`, `src/cli/prime.rs`, `src/cli/guard.rs`) |
| 17 | `src/cli/setup.rs` tests, calling `crate::test_support::language_marker_hits`, reading `rigger::prompt_surface::all_groups()` and the region entries directly |
| 18 | `tests/prompt_surface_sweep.rs` over the source tree and the two script files, reading `rigger::prompt_surface::all_groups()`, the module literals included (a qualifying module literal no group carries fails) |
| 19 | `gate.rs` tests (a forced guard-probe failure returns the `ProbeFault` error with the probe error and leaves no build directory behind; a probe over a missing parent directory is usable; a probe whose parent cannot be created returns the fault); `tests/reset_build_cache_periphery.rs` (the degraded-guard test rewritten to the fault); `conductor.rs` tests (a forced fault in one unit's inline gate ends the wave through `settle_stage`'s `HALT_MARKERS` arm with that `Err`, charging no attempt and recording no verdict while the pass's earlier verdicts stay; a forced fault in a deferred gate ends `conductor::run` with the same `Err` and records no deferred verdict; a stub runner's fault on the regenerate command returns that `Err` from `regenerate_conflicted_paths`) |
| 20 | the lanes script on the integrated tree |

The registry tests of criteria 16, 17 and 18 that read `all_groups()` run on every lane the store predicate
admits (default and no-default), never `core`, gated by the lib's store predicate as its outer modules are; the `core` lane compiles the domain's
and the binary crate's `prompt_surface` modules alone and runs only the domain module's own tests
(the binary crate is linted `--lib` there, `.rigger/gates/lanes.sh:70-72`; its module's tests run on
the default and no-default lanes).

ARTIFACT OWNERSHIP (owner = the criterion of the first production caller):

| Artifact | Owner |
| --- | --- |
| the pointer's framing paragraph, the Fan out section's two sentences and the cut of its "same rule" sentence | 1 |
| `Defaults.grep_guard`, the flag's reading in `resolve_spawn_host`, `carries_session_settings`' flag, the `cmd_setup` workflow parse, `session_hooks_advisory`'s guard clause and its stale-hook advice, the guard arms of `install_session_settings` and `install_lookup_hook` (the drifted-matcher replacement and the off-path's no-create read included), the scaffold's `grep_guard` line, `GREP_GUARD_MESSAGE`'s method-clause cut, its pin, `cmd_setup`'s removal line and its pin, the edited `tests/cli.rs:35001` assertion, the conditioned using-rigger sentence (`docs.rs:190-198`) and the two re-rendered `using-rigger` files | 2 |
| `hooks::resolved_helpers`, the `repo_root` argument at the six resolution points (`cmd_replay`'s, inside `materialize_config_at_rev` at the `--against` checkout root, included), `SpawnHost.helpers` (the test literals included), `HelperCopy` with its `present` marker, the `helpers` parameter of `definition_hash` folding the present entries, and the one read at each of its three callers (`cmd_step`, `fresh_run_if_requested`, `materialize_config_at_rev`), the `host.helpers` read inside `spawn_config_args`, `allowed_tools_args` taking `&SpawnHost` with its argument at both `build_args` callers, the validate check of the copies | 3 |
| the production-boundary reader `tests/common/fixtures/production_boundary.rs` and the move of the production-line pin (`opens_test_module`, the `lines[..end]` cut in `src/cli/setup.rs`'s test module) onto it (criterion 18 is its second caller and does not define it) | 2 |
| `SpawnHost` with `grep_guard`, `hooks::resolve_spawn_host(defaults)`, `Deps::spawn_host`, the `RunCtx` accessor, the guard flag at the six resolution points (`cmd_step` before `enforce_definition_pin`, `run_cli`, `run_workflow`, `cmd_replay` inside `materialize_config_at_rev` from the candidate config's defaults, the critic, the canary), `SpawnOpts.host`, `SpawnOpts::new`, every construction site's edit, `spawn_config_args`' `host: &SpawnHost` parameter passing `host.grep_guard` to `install_session_settings` with both `build_args` callers passing `&opts.host`, the no-`SpawnHost {` assertion | 2 |
| the cut cadence and method clauses of `EMIT_PROTOCOL`, `review_protocol`, `DECISIONS_HEADER`, `CODE_NEIGHBORHOOD_HEADER`, `DESIGN_INTENT_HEADER`, the lessons and findings headers, `notes_block`'s closing and `renderPeers`, the two `working-discipline` landings and their pins (`WHEN_YOU_REVIEW` and `ADVERSARY_FINDINGS_PROTOCOL`'s two cut clauses included; that const's build-cache sentence is criterion 11's) | 4 |
| the four hosts' progress texts, the `communication` built-in's progress bullet, the extended `communication` body pin | 5 |
| `DOC_ONLY_ROUND`'s fact text, the sdet-author persona's section | 6 |
| `Defaults.max_unit_added_lines`, `unit_size_cap(cap)`, both prompts' cap sentence, the `{size_cap_cause}` slot and its fill, the moved second sentence in the planner, adversary and adjudicator personas and their scaffold templates, the scaffold's cap line, the pins of `unit_size_cap` and `MAX_UNIT_ADDED_LINES` | 7 |
| `PLAN_PROTOCOL`'s guidance cut and MARKERS rewordings, the planner persona's and seed's moved sentences | 8 |
| both PATTERN lines, the pin of the hand-over section's bounded-audit bullet | 9 |
| `Defaults.test_author`, its refusal, the spawn's lookup, the edited sdet fixtures | 10 |
| `WaveItem::build_cache_env`, `spawn::BUILD_CACHE_ENV` and its four rewired readers (`RunCtx::spawn_env`, `gate.rs:608` and :656,
`spawn_request`'s env lookup), `spawn_env`'s no-sibling case, BUILD LOCATION's render rule (rendered only with a cache path), `REVIEWER_BUILD_CACHE` and `review_protocol`'s conditional render with its two fixtures, the removal of the build-cache sentence from `tests/critique_method_persona_periphery.rs`'s `ADVERSARY_FINDINGS_PROTOCOL` (the no-sibling plan-critique adversary), the script's reworded texts, the courier command and their tests | 11 |
| `ProbeFault` as `Runner::run`'s error variant, the degraded branch, its propagation at the three callers (`run_gates_at`, `run_deferred_gates`, `run_regenerate_command`) as a `HALT_MARKERS` halt, the marker fold `run_deferred_gates` shares with `settle_stage`, the F3 bypass, the kept verdicts, `ReplayRunner`'s always-`Ok` return, the probe's parent-directory create with the two rewritten degrade tests, `PROBE_FAULT_MARKER` and its membership in `HALT_MARKERS`, and their tests | 19 |
| the `rigger_scratch` description | 12 |
| each open-exceptions item's strike | the criterion EXCEPTIONS LIST names |
| `DocsOverlay.global_constraints`, the `DocsContext` field, `planning-a-spec` as `Rendered`, `.rigger/docs-overlay.yml`, the re-rendered `skills/planning-a-spec/SKILL.md`, the edited static-body tests | 13 |
| the shadow rule in `install_skills` and its report lines | 14 |
| `SetSpec.implementer`, `scaffold/rust/set.yml`'s persona, the `implementer` seed, `@IMPLEMENTER@`, the edited seed-set test | 15 |
| `rigger_domain::prompt_surface` (`Group`, `fill`, `INSTRUCTIONS_HEADING` with the re-path of its readers, `compose`, `render`, the prime line and the test fixtures, `instructions::HEADING` gone), the prime renderer's move into `src/cli/prime.rs` and its groups, THE CLASS TABLE's doc-comment class words, the `REVIEWER_BUILD_CACHE` row and the `REVIEWER_DISCIPLINE` row's narrowed contents, the hook rows (`GREP_GUARD_MESSAGE`, `GUARD_WRITE_DENY` and `rigger guard-write`'s exit-2 texts), its Const column and the per-crate source test pinning the (const, class) pairs, each crate's `prompt_surface` module with its texts and `prompt_groups()`, the lib's `pub mod prompt_surface` and `all_groups()`, every renderer's move onto it, the re-pathed readers, the helper's registry entries | 16 |
| the phrase list, the MARKERS call over the registry, the negative fixture, the removal of the emptied open-exceptions list and its lead-in | 17 |
| the sweep, the module-literal check (every qualifying literal inside a `prompt_surface` module carried by some group), the prompt-bearing derivation, its allow-list, the script regions (operator regions included), the region reader and their helper entries, the hook-text classification sentence | 18 |
| the lanes | 20 |

## Global constraints

- Hyphens, never em or en dashes, ASCII only, in every added line.
- A line citation names the symbol at the tree it was read on (8e06d136, on `rigger-run`); the symbol
  governs, the line is a pointer.
- No new event type; no new dependency.
- Every lane `.rigger/gates/lanes.sh` runs green (clippy -D warnings, test) and `cargo fmt --check`
  passes on the tree.
- The planner's batch order is UNIT ORDER's sequence, never checkbox position.
- No line this spec adds to any binary prompt string or embedded script prompt names a language or a
  toolchain; an interpolation identifier (`${...}`) is the wire contract's field name, not prompt
  text, exactly as criterion 17's MARKERS assertion strips interpolations.
- Every moved sentence lands verbatim in each home its Design block names before the binary copy is
  cut, the slot and duplicate rules of WHAT MOVES AND WHAT STAYS applied.
- Every new workflow key is absent-safe: an existing workflow.yml without it loads and behaves as
  declared, not as before.
- No unit edits `.rigger/workflow.yml` (OPERATOR HAND-OFF).

## Done when

- [ ] a test proves THE LOOKUP POINTER NAMES ONLY THE TOOLS: `write_lookup_pointer` names the three graph verbs and their jobs, and its "not by grepping" and `grep-fallback:` record sentences stand verbatim in the `working-discipline` built-in's Fan out section. This criterion OWNS the pointer's text, that section's added sentences and their pins; the guard hook is criterion 2's, NOT this one's.
- [ ] a test proves THE GREP GUARD IS DECLARED: `hooks::install_session_settings` merges the PreToolUse grep-guard only when `defaults.grep_guard` is true, as `hooks::resolve_spawn_host` gives it to every spawn `SpawnOpts::new` builds, critic and canary included, and `install_lookup_hook` removes only rigger's own entry when absent. This criterion OWNS the key, `SpawnHost` and both installers' guard arm; the helpers are criterion 3's, NOT this one's.
- [ ] a test proves THE OPERATOR'S HELPER COPY IS THE DEFINITION: `SpawnHost.helpers` holds `<root>/.claude/agents/lookup.md` and `verify.md` if present, else the embedded `HELPER_AGENTS`, resolved once per step, critic or canary run; `spawn_config_args` builds `--agents` and `allowed_tools_args` the pre-approval from it. This criterion OWNS `resolved_helpers`, `helpers`, its filling and both readers; `SpawnHost` itself is criterion 2's, NOT this one's.
- [ ] a test proves THE TASK PROMPTS CARRY NO CADENCE OR METHOD: `EMIT_PROTOCOL`, `review_protocol`, the grounding sections' headers, the run notes' closing and the shim's `renderPeers` keep their shapes and nouns with every cadence or method clause Design names cut, a built-in stating each. This criterion OWNS those texts' cuts, the moved `working-discipline` sentences and their pins; the progress cadence is criterion 5's, the planner's clause criterion 8's, NOT this one's.
- [ ] a test proves THE PROGRESS CADENCE HAS ONE HOME: the process hosts' `progress_framed`, the workflow script's and the shim's progress notes and the `rigger_progress` description name the verb and its arguments only, and the cadence wording Design names stands in the `communication` built-in. This criterion OWNS the four host texts, the built-in's bullet and their pins; other cadence is criterion 4's, NOT this one's.
- [ ] a test proves A DOC-ONLY ROUND STATES ONLY THE FACT: the sdet-author prompt on a round `is_documentation_only` classifies carries "This round changed documentation only." and no rule, and the rule's sentence stands verbatim in `.rigger/agents/sdet-author.md`. This criterion OWNS `DOC_ONLY_ROUND`, its doc, the persona's section and their pins; the test-author key is criterion 10's, NOT this one's.
- [ ] a test proves THE UNIT SIZE CAP IS DECLARED: `unit_size_cap` renders its first sentence from `defaults.max_unit_added_lines` in the DAG critique prompt and `PLAN_PROTOCOL` only when declared, neither carrying a cap sentence when absent, `MAX_UNIT_ADDED_LINES` gone and the second sentence in the planner and both critique personas. This criterion OWNS the key, the cap text and its pins; other planner guidance is criterion 8's, NOT this one's.
- [ ] a test proves THE PLANNER PROTOCOL CARRIES NO PLANNING POLICY: `PLAN_PROTOCOL` holds exactly the sentences Design keeps as framing, and every sentence Design moves (the split, discovery, scope-creep, cadence, no-code and omit-the-id sentences) stands verbatim in this repository's planner persona and the scaffold's planner seed. This criterion OWNS both lists and their pins; the cap sentence is criterion 7's, NOT this one's.
- [ ] a test proves THE REQUIRED PATTERN NAMES ONLY ITS RECORD: `PriorFailure::block` and `ReviewRound::block` give each REQUIRED pattern item its shape and its `audit_id` record id, and the bounded-audit bullet stands verbatim in the `working-discipline` built-in's hand-over section. This criterion OWNS both PATTERN lines and their pins, that bullet's pin included; the lookup sentences in that body are criterion 1's, NOT this one's.
- [ ] a test proves THE TEST AUTHOR IS DECLARED: `spawn_sdet_author` spawns the agent `defaults.test_author` names, under the `sdet-author` role token, and spawns nothing when the key is absent, whatever `.rigger/agents` defines; an undefined id is refused at load. This criterion OWNS the key, its refusal, the spawn's lookup and the edited fixtures; the doc-only rule is criterion 6's, NOT this one's.
- [ ] a test proves THE WORKFLOW SCRIPT NAMES NO TOOLCHAIN: `RIGGER_WORKFLOW`'s worker and courier texts name the cache only through `WaveItem::build_cache_env` and the wave's path, without the disk-layout sentence, courier prefix or "the moment", and a spawn with no unit sibling carries no cache variable. This criterion OWNS those texts, the wave field, the no-sibling case and their pins; `rigger_scratch` is criterion 12's, NOT this one's.
- [ ] a test proves THE SCRATCH TOOL NAMES NO TOOLCHAIN: the `rigger_scratch` description that `crates/rigger-dash/src/mcpserver.rs`'s `tool_list` advertises says "build cache" where it named the toolchain's cache variable and "Reaped once your result records.", keeping the reaped container's contract word for word otherwise. This criterion OWNS that description and its pins; the workflow script's texts are criterion 11's, NOT this one's.
- [ ] a test proves THE PLANNING SKILL CARRIES THE PROJECT'S CONSTRAINTS: `planning-a-spec` step 6 reads "Carry the project's own Global constraints" with neutral path examples, and the `global_constraints` list of `DocsOverlay` renders under it in the installed copy. This criterion OWNS the body's wording, the overlay field and this repository's overlay file; skill shadowing is criterion 14's, NOT this one's.
- [ ] a test proves A PROJECT SKILL SHADOWS THE SHIPPED ONE: `install_skills` installs `.rigger/skills/<name>/SKILL.md` verbatim in place of the shipped skill of that exact name, a near name shadowing nothing, while `rigger docs` and the drift gate stay on the shipped text. This criterion OWNS the shadow rule and its setup report lines; the skill bodies are criterion 13's, NOT this one's.
- [ ] a test proves THE IMPLEMENTER COMES WITH THE GATE SET: `init_project` with the Rust set writes its `implementer` persona and names that id in `SCAFFOLD_WORKFLOW`'s implement and checkin stages, and with no set writes and names a language-neutral `implementer`. This criterion OWNS `SetSpec.implementer`, the seed, the placeholder and their pins; the scaffold's keys are criteria 2 and 7's, NOT this one's.
- [ ] a test proves THE REGISTRY HOLDS EVERY PROMPT TEXT: each crate's `prompt_surface` module holds its crate's prompt texts as `Group`s whose doc comments open with the class Design's table assigns, every renderer formats through `fill`, and each surface's groups concatenate to its render over the placeholder fixture. This criterion OWNS the types, every crate's module and the concatenation tests; the phrase test is 17's, the sweep 18's, NOT this one's.
- [ ] a test proves NO REGISTERED TEXT PRESCRIBES: every group `prompt_surface::all_groups()` returns and every joined script region entry carries no phrase of the test's cadence and method list and no hit under the MARKERS rule (`language_marker_hits`), and a held pre-move `EMIT_PROTOCOL` fails it. This criterion OWNS the phrase list, both assertions and removing the emptied exceptions list; the registry is criterion 16's, NOT this one's.
- [ ] a test proves THE SOURCE SWEEP CLOSES THE REGISTRY: every production literal of 40 or more characters with a space in a prompt-bearing file (one spelling `SpawnOpts`, `SpawnHost` or `prompt_surface`) is registered or allow-listed, and each such literal of the two embedded scripts sits in a classed region. This criterion OWNS the sweep, the file derivation, the allow-list and the regions; the registry is criterion 16's, NOT this one's.
- [ ] a test proves THE FAILED GUARD PROBE HALTS THE STEP: a forced guard-probe failure makes `Runner::run` return `ProbeFault` naming the guard path and OS error, and the inline, deferred and regenerate callers each end the step with it as a `HALT_MARKERS` halt, with no gate verdict, no F3 rerun, no charged attempt and the pass's earlier verdicts kept. This criterion OWNS `ProbeFault` and its propagation; the courier prefix is criterion 11's, NOT this one's.
- [ ] `.rigger/gates/lanes.sh` runs green for both feature lanes on the finished tree, verified by `sh .rigger/gates/lanes.sh no-default` and `sh .rigger/gates/lanes.sh core` each exiting 0, with `cargo fmt --check` passing. This criterion OWNS only the lanes; every behaviour is criteria 1 to 19's, NOT this one's.
