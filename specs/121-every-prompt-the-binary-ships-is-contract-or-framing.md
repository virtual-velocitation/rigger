# 121 - Every prompt the binary ships is contract or framing

**Goal:** every prompt text and tool description the binary puts in front of an agent as
instruction is CONTRACT (a shape the binary parses,
a tool name or argument) or FRAMING (a fact of the task only the binary knows); a PRESCRIPTION
(method, cadence, discipline, review or test-authoring procedure, planning policy, a language or
toolchain word) lives in a surface the consumer owns: the overridable built-in layer (spec 120), the
personas (this repository's `.rigger/agents/*.md` and the scaffold seeds), the skills, or a declared
workflow key. The prompt-surface audit of 2026-10-20 (`~/.cache/rigger/drafts/op/prompt-surface-audit.md`,
read at eb0f8718) found the prescriptions spec 120 leaves in the binary, each with its homes. The
graph-first lookup method has three homes: `write_lookup_pointer` (`conductor.rs:13422`, in every
grounded task prompt, a mandatory `grep-fallback:` record included), the PreToolUse grep-guard
(`hooks.rs:25-27`, merged into every headless spawn's `--settings` by `spawn_config_args`,
`driver/mod.rs:288`, and into the operator's `.claude/settings.json` by `install_lookup_hook`,
`src/cli/setup.rs:1147`) and the `lookup` helper. The helpers have two homes on the headless hosts
and the binary's wins: `HELPER_AGENTS` (`hooks.rs:40-49`) is passed inline, so an edited
`.claude/agents/lookup.md` never reaches a headless spawn. The communication cadence has four homes
outside `RIGGER_COMMUNICATION`: `EMIT_PROTOCOL` "the moment you make it" (:12424), `review_protocol`
"the moment you raise it" (:12508), `DECISIONS_HEADER` "do not contradict them; supersede explicitly
if you must" (:12663) and the shim's "Re-check the rigger_peers tool between your own actions"
(`shim/shim.mjs:150`), and a fifth the audit missed, `PLAN_PROTOCOL`'s "Propose each refinement the
moment you decide it" (:12435). The progress cadence has four host homes in four wordings:
`driver/cli.rs:146-151` (every cli-host spawn), `workflows/rigger.js:649-652` (every workflow-path
worker), `shim/shim.mjs:253-256` (every shim worker) and the `rigger_progress` description
(`mcpserver.rs:761`, the stream host). `DOC_ONLY_ROUND` (:13842) carries a test-authoring rule whose
doc says it "lives here alone, never copied into a persona". `MAX_UNIT_ADDED_LINES` (:12413) is a
planning policy as a constant every project gets. `PLAN_PROTOCOL` carries two planning rules the
code does not enforce (an unmatched unit still runs). The REQUIRED PATTERN audit (`conductor.rs:1402`,
:12572-12575) is a procedure asked and checked by prompts only. `ROLE_SDET_AUTHOR` (`spawn.rs:105`)
adds a spawn by fixed agent id that no workflow key declares (`spawn_sdet_author`,
`conductor.rs:5208`, :5229). `workflows/rigger.js` names `cargo` (:624, :659, :893), a mutation tool
(:659) and one operator machine's disk layout (:673), and hard-codes a cache path (:893);
`rigger_scratch` (`mcpserver.rs:762`) names `CARGO_TARGET_DIR`. The `planning-a-spec` skill
(`crates/rigger-domain/src/docs.rs:426`) carries this repository's house constraints and Rust path
examples, and every shipped skill is rewritten on drift, so no consumer can replace one. The scaffold
names `agent: rust-engineer` (`src/cli/setup.rs:1590`, :1605) and seeds the Rust persona whichever
gate set matched. And nothing stops a new prescription entering a prompt: no test enumerates the
prompt surface. This spec moves each prescription to its owner's surface and lands a boundary gate
that fails on any string literal of 40 or more characters with a space in a prompt-bearing file that
is neither registered as contract or framing nor allow-listed as an operator message. At 3cee73df the
production text (outside `tests/`, cut at each file's test module) of `crates/` and `src/` holds 1062
such literals in 59 files; the prompt-bearing subset (files spelling `SpawnOpts`, `SpawnHost` or
`prompt_surface`) holds 124 in 8 files (`conductor.rs` 95, the headless host driver module of `rigger-driver` 10, `critique.rs` 6,
`canary_store.rs` 5, `gate.rs` 3, `driver/cli.rs` 2, `driver/mod.rs` 2, `agent.rs` 1), a set that grows
by every renderer file criterion 16 moves onto `prompt_surface`: after it, 14 files holding 281 such
literals at 3cee73df's count (the 8 above plus `src/cli/mod.rs` 91, `mcpserver.rs` 38, `src/cli/guard.rs`
14, `review.rs` 11, `src/cli/prime.rs` 2, the prime renderer's literals criterion 16 moves out of
`src/cli/setup.rs`, and `crates/rigger-driver/src/hooks.rs` 1, which spells `SpawnHost` once the
resolver lives there; `src/cli/run.rs` and `src/cli/eval.rs` call the resolver and pass its value
without naming the type, so they stay outside), and each crate's new `prompt_surface` module, whose literals are the registry
itself. Tool results, errors and CLI output an agent reads back are data, not instruction, and are
outside this claim (THE SOURCE SWEEP's tool-output category).

## Design

**BASE, decided here.** Spec 121 lands on `rigger-run` after spec 120 has integrated there (spec 120
itself after PR #117). Its code citations are read at 3cee73df (`rigger-run`) and its spec 120 facts
at spec 120's text as of 0ecfd8a8; the symbol governs and the line is a pointer. On the launch base,
by spec 120: `RIGGER_COMMUNICATION` and the free function `build_system_prompt` are gone
(`RunCtx::build_system_prompt` stays) and the `communication` built-in carries the discipline (its
criterion 14); `REVIEWER_DISCIPLINE` holds three sentences and is `pub`; `gate_evidence_block`
holds only its rows and framing sentence (its criterion 13); the critique method constants are gone
(its criterion 15) and the method stands in the adversary's and the adjudicator's `## When you
critique a spec or a plan` sections, persona and scaffold template; the adversary's section holds the
size rule with no number ("a unit over the cap your prompt states is a BLOCKING finding") and the
adjudicator's judges "a unit over the cap the prompt states"; the DAG critique prompt renders
`unit_size_cap()`'s text verbatim as its cap framing, `unit_size_cap` and `MAX_UNIT_ADDED_LINES`
unedited; the MARKERS rule exists in its criterion 12's test. PR #117's shipped-text helper `every_shipped_agent_text()` (a test helper in
`src/cli/setup.rs`, at PR #117's head cdb6de67, :3439) exists. The `working-discipline` body's `lookup`
bullet carries, verbatim, "The same rule binds you: the graph first, a literal `grep` only when it is
strictly necessary, and each such use recorded." (criterion 1 cuts it). The launch record confirms each of
these before a unit spawns, that the open-exceptions list in `authoring-loops.md`'s `## What rigger
prescribes` section (spec 120's criterion 1) names exactly the prescriptions EXCEPTIONS LIST assigns
to this spec's criteria, in the wording spec 120 landed (no amendment to spec 120), and that the HIT ENUMERATION below is complete (no phrase-list or
MARKERS hit in a registry surface or script prompt at the launch base outside it); a mismatch stops
the launch for a Design amendment.

**WHAT MOVES AND WHAT STAYS, decided here.** Each moved sentence lands verbatim in its new home in
the same commit that cuts the binary copy. A sentence carrying a slot lands with the slot replaced by
the noun phrase this Design names. Where one prescription has several homes in different words, the
wording named here lands and the other homes' wordings are cut as its duplicates. The binary keeps
every CONTRACT and FRAMING part: tool names and arguments, event shapes, the verdict and finding
grammar, the `required` list, the self-report, heartbeat and scratch protocols, the unit, its
criterion, paths, gate evidence, the REQUIRED list, the delta and the grounding sections.

EXCEPTIONS LIST, decided here: spec 120 lands in `authoring-loops.md`'s `## What rigger prescribes`
a list of the prescriptions this spec still owns, the rule's open exceptions. Each item is struck by
the criterion that lands it, in the same commit: the lookup pointer's method sentences by 1; the
cadence clauses in `EMIT_PROTOCOL` and `review_protocol` by 4; the progress cadence in the four host
homes by 5; `DOC_ONLY_ROUND`'s rule sentence by 6; `MAX_UNIT_ADDED_LINES` and `unit_size_cap`'s
second sentence by 7; `PLAN_PROTOCOL`'s guidance by 8; the REQUIRED PATTERN audit sentences by 9; the
shared item "the toolchain words in the workflow script and the scratch tool" by 11 and 12 together:
a list item shared by two criteria is NARROWED by the first of them to land, to the remaining concern
("the toolchain words in the scratch tool" when 11 lands first, "the toolchain words in the workflow
script" when 12 does), and STRUCK by the second. Criterion 17, which lands after every striking
criterion (1 to 15, then 16 and 18), removes the emptied list with its lead-in sentence; each striking
criterion keeps only its own strike.

HIT ENUMERATION, decided here: every phrase-list hit (criterion 17's list) and every MARKERS hit
(spec 120's rule) in a registry surface or a script prompt at 3cee73df, with its disposition, by one
rule: a hit inside a prescription moves with that prescription to its named home (or is cut as the
duplicate of a home that already states it), never reworded; a hit inside a framing or contract
sentence is reworded with the replacement named here, since the sentence prescribes nothing.

| Site | Hit | Disposition |
| --- | --- | --- |
| `conductor.rs:1402` `PriorFailure::block` PATTERN | "audit the whole tree" | prescription, moves to `working-discipline` (criterion 9) |
| `conductor.rs:12573` `ReviewRound::block` PATTERN | "audit the whole tree" | prescription, moves (criterion 9) |
| `conductor.rs:12424` `EMIT_PROTOCOL` | "the moment" | prescription, cut as the `communication` built-in's duplicate (criterion 4) |
| `conductor.rs:12508` `review_protocol` | "the moment" | prescription, cut as duplicate (criterion 4) |
| `conductor.rs:12664` `DECISIONS_HEADER` | "do not contradict", "supersede explicitly" | prescription, cut as duplicate (criterion 4) |
| `conductor.rs:12435` `PLAN_PROTOCOL` | "the moment", "do not write code", "scope creep" | prescriptions, move to the planner persona and seed (criterion 8) |
| `conductor.rs:12435` `PLAN_PROTOCOL` | MARKERS `matches` (twice) | framing sentences, reworded "pairs your unit with its baseline" and "is none of the ids below" (criterion 8) |
| `conductor.rs:13424` `write_lookup_pointer` | "not by grepping" | prescription, moves to `working-discipline` (criterion 1) |
| `conductor.rs:13842` `DOC_ONLY_ROUND` | "never a new test" | prescription, moves to the sdet-author persona (criterion 6) |
| `driver/cli.rs:148` `framed` | "keep it flowing" | prescription, cut as duplicate of the moved progress bullet (criterion 5) |
| `mcpserver.rs:761` `rigger_progress` | "never batch" | prescription, cut as duplicate (criterion 5) |
| `mcpserver.rs:762` `rigger_scratch` | "the moment"; MARKERS `cargo` (`CARGO_TARGET_DIR`) | contract sentences, reworded "Reaped once your result records." and "build cache" (criterion 12) |
| `rigger.js:650-652` `progressNote` | "do not batch", "keep it flowing" | prescription, moves to the `communication` built-in (criterion 5) |
| `rigger.js:624` `workdir` | MARKERS `cargo` | framing, reworded "your build tool's runs" (criterion 11) |
| `rigger.js:659-660` BUILD LOCATION | MARKERS `cargo`, `clippy` (and "mutants"), `CARGO_TARGET_DIR` | framing, reworded "every command your build tool runs ... (anything that builds or tests)" and `${req.build_cache_env}` (criterion 11) |
| `rigger.js:661` BUILD LOCATION | `target/` (the toolchain's default build directory) | framing, reworded "A build cache inside the worktree itself is a defect that fills the disk - never create one." (criterion 11) |
| `rigger.js:673` SCRATCH POLICY | "the moment"; MARKERS `cargo` (twice) | the disk-layout parenthetical is a prescription, dropped (the reaper is its contract); the reaper sentence is contract, reworded "It is reaped once your result records"; the export sentence is framing, reworded with `${req.build_cache_env}` (criterion 11) |
| `rigger.js:892-894` step courier | MARKERS `cargo`, `CARGO_TARGET_DIR` | framing; the prefix and its sentence dropped (the binary sets the variable on every branch), "running their gates inline" (criterion 11) |
| `rigger.js:55` phase detail | MARKERS `cargo` | not a prompt (the `/workflows` phase label): an operator region (criterion 18), stays as written |
| `shim.mjs:149-150` `renderPeers` | "do not contradict", "supersede explicitly", "between your own actions" | prescription, cut as duplicate (criterion 4) |
| `shim.mjs:253-256` `progressNote` | "do not batch", "keep it flowing" | prescription, cut as duplicate (criterion 5) |

Spec 120 disposes of the rest before the launch base: `gate_evidence_block` (`conductor.rs:592`,
MARKERS, its criterion 13), `REVIEWER_DISCIPLINE` (:12529, its criterion 12), `RIGGER_COMMUNICATION`
(:12446, its criterion 14, a built-in, never a registry entry), the DAG prompt's "Reject ONLY"
(:7927) and `CRITIQUE_STANCE`'s "default to skepticism" (`review.rs:295`, its criterion 15). No other
registry surface (`GREP_GUARD_MESSAGE`, the canary's texts, `guard.rs`'s deny reason, the other MCP
descriptions) carries a hit.

**THE LOOKUP METHOD, decided here (criterion 1).** `write_lookup_pointer` becomes one framing
paragraph: "The knowledge graph answers lookups through three verbs:" followed by the three verbs
with their one-line jobs as at HEAD. Its two guidance sentences move verbatim into the
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
CONTRACT shape (`progress::GREP_FALLBACK_PREFIX`, `crates/rigger-domain/src/progress.rs:84`, which
`metrics.rs:1147` and the dash count); nothing in the binary demands it. The existing pin
(`conductor.rs:19406-19410`, "the pointer must carry the grep-fallback reporting instruction") moves
to the built-in body. The `using-rigger` skill's sentence (`docs.rs:184`) stays: a skill is the
consumer's surface (criterion 14).

**THE GREP GUARD IS DECLARED, decided here (criterion 2).** `Defaults` (`config.rs:523`) gains
`grep_guard: bool`, default false. Declared true: `rigger setup`'s `install_lookup_hook` installs the
PreToolUse entry as at HEAD, and every headless spawn's `--settings` carries it. Absent or false:
`install_lookup_hook` removes rigger's own entry from `.claude/settings.json`: every inner hook
under `PreToolUse` whose command equals `GREP_GUARD_COMMAND`, whatever its matcher, a block dropped
only when its inner array becomes empty, an emptied `PreToolUse` array and an emptied `hooks` object
removed in turn, so a settings file rigger alone populated returns to its pre-rigger shape (the
byte-equal oracle: a settings file holding the guard plus foreign hooks equals, after removal, the
file `rigger setup` writes from the same input with the key off), every other hook kept, writing only when the merge changes
the file. Install keys on the same rule: the guard is present when any inner `PreToolUse` hook has
that command, and an install with the key true replaces every inner hook carrying
`GREP_GUARD_COMMAND` whose matcher differs from `GREP_GUARD_MATCHER` (`hooks.rs:25`), so exactly one
entry with the current matcher remains (`cmd_setup` parses `.rigger/workflow.yml` after `init_project` to read the
key and fails loudly on a parse error, as `get_referenced_agent_ids`, `setup.rs:189`, already does); and `hooks::install_session_settings` takes the flag and merges only the
prime hook and the status line. Corners: with the key off the removal reads `settings.json` only if it exists and
creates no `.claude/` directory (it never reaches the settings installer's directory creation,
`setup.rs:1164`), so an absent `settings.json` with the key off writes nothing and
reports `AlreadyCurrent`; a removal that changes the file reports `Refreshed`;
`hooks::carries_session_settings` takes the same flag, which its caller (`src/cli/validate.rs:824`)
reads from the loaded config; `session_hooks_advisory` (`src/cli/validate.rs:822-834`) names the
grep-guard among the missing hooks only when `defaults.grep_guard` is declared, and names a present
`GREP_GUARD_COMMAND` hook as stale when the key is false, telling the operator to run `rigger
setup`; criterion 2 owns both. A spawn's flag comes
from SPAWN HOST RESOLUTION, never set by hand;
`spawn_config_args` passes it through. `cmd_grep_guard` reads no key: a `settings.json` written while
the key was true keeps denying after the key flips false until the next `rigger setup`, the operator's
standing step after any workflow key change; accepted. The hook mechanism, `rigger grep-guard` and
`GREP_GUARD_MESSAGE` stay. `SCAFFOLD_WORKFLOW` declares `grep_guard: true` under `defaults`, so a
freshly scaffolded project behaves as at HEAD (SCAFFOLD DEFAULTS).

**SPAWN HOST RESOLUTION, decided here (criteria 2 and 3).** The host values a headless spawn
needs, the guard flag and the helper texts, are resolved by one function,
`rigger_driver::hooks::resolve_spawn_host`, into one value, `agent::SpawnHost`, a struct beside
`SpawnOpts` (`crates/rigger-domain/src/agent.rs:64`) of `grep_guard: bool` (from
`defaults.grep_guard`) and `helpers: Vec<(String, String)>` (name, text). The resolver lives in
`rigger-driver` beside `hooks::resolved_helpers` because the domain crate reaches neither the loaded
`Config` (`rigger-config-files`) nor the filesystem; it takes `&Defaults`
(`crates/rigger-domain/src/config.rs:523`), which every caller holds as `cfg.workflow.defaults`.
Criterion 2 lands `resolve_spawn_host(defaults: &Defaults) -> SpawnHost` with the `grep_guard` field
only, so no unit lands a parameter nothing reads; criterion 3 adds the `repo_root: &Path` parameter,
consumed inside the resolver (the root is no field), and the `helpers` field it fills once through
`hooks::resolved_helpers(repo_root)`, the same value `definition_hash` folds, the return becoming
`Result<SpawnHost, Error>`: a copy that cannot be read or parsed fails the resolver, the
infrastructure fault THE OPERATOR'S HELPER COPY decides. `SpawnOpts` gains one field, `host:
SpawnHost`, and every construction sets it only from a resolved value, never a field by hand: the
five conductor sites (`conductor.rs:4519`, :5758, :6384, :8011, :10760) through one `RunCtx` accessor
that only reads a named `Deps` field, `Deps::spawn_host`, and never resolves. `rigger step`
(`cmd_step`, `src/cli/run.rs:428`) calls `resolve_spawn_host` ONCE, before `enforce_definition_pin`
(`src/cli/run.rs:162`), and hands that value to the conductor in `Deps::spawn_host`, so every spawn
the step parks or runs carries that one value; `rigger critique`'s (`src/cli/critique.rs:183`) and
the canary's (`canary_opts`, `canary_store.rs:571-577`) values are resolved once at their start in
`src/cli` the same way (`cmd_critique`; `src/cli/eval.rs:228`, which hands the value to `run_canary`
as a parameter). Criterion 2 resolves from `defaults` alone at each of the three points; criterion 3
adds the `repo_root` argument there (the step's and the canary's repository root, the critic's
`self.root`). These are every `SpawnOpts { .. }` construction in
production code at 3cee73df. `SpawnHost` derives no `Default` and its fields are `pub`; `SpawnOpts`
loses its `Default` derive (`agent.rs:63`) and gains `SpawnOpts::new(host: SpawnHost) -> SpawnOpts`,
filling every other field with its former default, so every construction spreads
`..SpawnOpts::new(host)` and the compiler enumerates every site, tests included (a test builds its
host as a struct literal; criterion 3 adds `helpers` to every such literal). A test asserts that no
production line outside the body of `resolve_spawn_host` (the lines from `fn resolve_spawn_host` to
its closing brace at the same indent, exempt) constructs a `SpawnHost` literal (a production reader
accesses the fields, `host.grep_guard` and `host.helpers`, and never destructures a `SpawnHost`, so
every match is a construction): it matches `SpawnHost {` followed, after optional whitespace or a
newline, by a field name or `..`, and never preceded on its line by `->`, `struct ` or `impl `, so
the declaration, the impl headers and the resolver's and the `RunCtx` accessor's signatures are
exempt by that rule. Criterion 2 owns `SpawnHost` with its `grep_guard` field, `resolve_spawn_host`,
`SpawnOpts.host`, `SpawnOpts::new`, `Deps::spawn_host`, the `RunCtx` accessor, the three resolution
points (`rigger step`'s once-per-step call before the pin check, the critic's and the canary's), the
edit of every construction and that assertion, so its text holds on a tree without criterion 3;
criterion 3 adds only the `repo_root` argument, the `helpers` field, its fold into the hash and both
readers, `spawn_config_args` (the `--agents` JSON from
`host.helpers`) and `allowed_tools_args` (`crates/rigger-driver/src/driver/mod.rs:45-55`, the tool
pre-approval from the same value), so 3 needs 2. Each criterion's test proves the critic's and the
canary's spawns carry the resolved value.

**THE OPERATOR'S HELPER COPY IS THE DEFINITION, decided here (criterion 3).** One function,
`hooks::resolved_helpers(root: &Path) -> Result<Vec<(String, String)>, Error>`, returns each
`HELPER_AGENTS` entry's text: `<root>/.claude/agents/<file>` when that file exists, the embedded text
otherwise; `resolve_spawn_host` calls it once and carries its value as `SpawnHost.helpers`, the one
carrier. `helper_agents_json` and `helper_mcp_tools` (`hooks.rs:74`, :110) take that value:
`spawn_config_args` builds `--agents` from `host.helpers`, and `allowed_tools_args` builds the
pre-approval from the same value; both `build_args` callers, the headless host driver module of
`rigger-driver` and the cli host (`driver/cli.rs:273`, :288), pass the spawn's `SpawnOpts.host`, so
the `--agents` JSON and the tool pre-approval derive from one resolution. The root is the repository
root the run drives (for the conductor `Deps::repo`, `conductor.rs:1844`): the operator's checkout is
the one root every spawn reads, and a helper copy committed into a unit worktree is ignored (this
repository tracks `.claude/agents/`, `.gitignore:50`, and `hooks.rs:43` embeds the tracked file, so a
worktree copy is only ever the operator's own text at an older commit); an empty root reads the
embedded text. A present file that cannot be read or does not parse as a helper definition fails
`resolve_spawn_host` with an error naming the file, an infrastructure fault that charges no
remediation attempt; `rigger validate` reports the same error. Each operator copy that is PRESENT
folds into `definition_hash` (`src/cli/mod.rs:974`) after the agent definitions, from that one
resolution's `helpers`; an absent copy folds nothing, so the hash of a project without copies is unchanged and it
sees no drift. A mid-run edit of an operator copy is a definition drift that halts the step and
resumes with `--rebase-definition`, as a persona edit does. The copies are read once per step, in
`src/cli`: `rigger step` folds `definition_hash` from the `helpers` of the value `resolve_spawn_host`
returned before `enforce_definition_pin`, and hands that same value to the conductor in
`Deps::spawn_host`, so the hash and every spawn read one resolution and no second crate reads the
copies; criterion 3 owns the fold from that value and its tests. `HELPER_AGENTS` stays as the scaffold's
write-once seed (`setup.rs:243-244`) and the default.

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

**THE PROGRESS CADENCE HAS ONE HOME, decided here (criterion 5).** The wording that lands is the
workflow script's (`rigger.js:650-652`), the superset of the other three, as one bullet of the
`communication` built-in, inserted after its last existing bullet and before its closing sentence
("The exact JSON shapes for these emits are given in your task instructions; ..."), the pin's
expected text being the bullet's text: "- After each significant step - a search, a file read, a build, a
commit, a decision - report ONE short line of what you just did through the progress verb your task
or your tool list names. This is how an observer sees you working between the milestones you record, so a long silent
stretch is never mistaken for a stall. Keep it flowing WHILE you work; do not batch it at the end."
(the slot "from ${REPO}, using Bash" replaced by "through the progress verb your task or your tool list names"). Each
host keeps the verb and its arguments: `framed` (`driver/cli.rs:146-151`) appends "LIVE PROGRESS:
record a progress line by running (Bash): rigger progress '<id>' '<one line: what you just did>'.";
the script's `progressNote` reads "LIVE PROGRESS: record a progress line, from ${REPO}, using Bash:"
then the command line; the shim's `progressNote` (which has no `REPO` binding) reads "LIVE PROGRESS: record a progress line
by running (Bash):" followed by the `rigger progress '${next.id}' '<one line: what you just did>'`
line, as `framed` does; the `rigger_progress`
description reads "Record one live progress line for the spawn this server is bound to, so an observer
sees you working between milestones." ("Report one short line after each significant step; never
batch them up." cut). Criterion 5 owns extending spec 120's criterion 14 pin of the `communication`
body to the added bullet. The heartbeat's "after each significant step" stays: the liveness sweep reads
the marker (CONTRACT).

**A DOC-ONLY ROUND STATES THE FACT, decided here (criterion 6).** `DOC_ONLY_ROUND` becomes "This round
changed documentation only." `is_documentation_only` (`conductor.rs:13801`) stays as the fact's
classifier. The rule "Extend the existing owner test's needle for the changed sentence, or add no pin;
never a new test." (the clause after the colon, capitalized as a sentence) lands verbatim in
`.rigger/agents/sdet-author.md` under a `## A documentation-only round` heading after `## Account for
every item`. The constant's doc loses "The rule lives here alone, never copied into a persona." The
scaffold seeds no sdet-author persona and declares no test author (SCAFFOLD DEFAULTS), so this
repository's persona is the rule's one home; a consumer declaring a test author writes its own rule.

**THE UNIT SIZE CAP IS DECLARED, decided here (criterion 7).** `Defaults` gains
`max_unit_added_lines: u32`, default 0 (no cap). The key lives under `defaults` only: the planner and
the plan-critique gate both read it, and one home serves both. `MAX_UNIT_ADDED_LINES` is deleted;
`unit_size_cap(cap: u32) -> String` renders its first sentence from the key, "A unit is too large
when it is expected to add more than {cap} lines." (FRAMING: the declared number), in place of the
constant-based sentence and never beside it, and is called only when `cap > 0`. In both prompts that
render it, the DAG critique prompt (its cap framing, spec 120's) and `PLAN_PROTOCOL` (its
`{unit_size_cap}` slot, rendered as that sentence followed by a space), the cap sentence appears only
when the key is declared; absent, neither prompt carries a cap sentence. The function's second
sentence, "The cap measures review scope - how much one review round must read - never code shape."
(`conductor.rs:12420`), leaves the binary and lands verbatim in this repository's
`.rigger/agents/planner.md` and the scaffold's planner seed, its one home, directly after criterion
8's moved split sentence in both homes (so 7 needs 8). Because the sentence reaches the plan
critique today through the DAG prompt's cap framing, criterion 7 also lands it verbatim in the
adversary's and the adjudicator's `## When you critique a spec or a plan` sections, persona and
scaffold template alike, directly after each section's cap clause (the adversary's numberless size
rule, the adjudicator's "the cap the prompt states" clause, both spec 120's and otherwise
untouched), so no persona this repository or the scaffold ships loses it. Three role-voiced homes are not a duplicate under spec
120's one-home ruling: the home of a method is the persona of the role that applies it. The DAG prompt's cause-contract example "a unit over the
size cap" is rendered only when the key is declared. The binary keeps the `decomposition-conflict`
cause contract. Criterion 7 owns every pin of `unit_size_cap`'s text and of `MAX_UNIT_ADDED_LINES`
(`conductor.rs:46084-46103` among them). `SCAFFOLD_WORKFLOW`
declares `max_unit_added_lines: 3000` (SCAFFOLD DEFAULTS); this repository's workflow declares 3000
(OPERATOR HAND-OFF).

**THE PLANNER PROTOCOL CARRIES NO POLICY, decided here (criterion 8).** Every `PLAN_PROTOCOL`
sentence (`conductor.rs:12435`) is in exactly one of two lists, by one rule: a sentence stating what
the conductor does with the planner's output is FRAMING and kept; a sentence telling the planner how
to plan moves.

KEPT (FRAMING), each as at HEAD unless reworded here: "You are the planner. The conductor has ALREADY
created one baseline implement unit per acceptance criterion below - the spec is decomposed by
construction."; "Your job is to REFINE that baseline, not to re-decompose it."; the split bullet's
contract, reworded as its own sentence after the `{unit_size_cap}` slot: "Each part of a split echoes
the split criterion's id and ends its `criterion` text with an OWNS sentence naming its part (the conductor
keeps that sentence after the criterion's exact text as that unit's contract)."; "Each criterion below
is shown with a STABLE id in [brackets]."; "When your unit serves a criterion, your unit SUPERSEDES
(replaces) that criterion's baseline - it does NOT run alongside it - so identify the criterion you
serve by ECHOING its id: ..." (to the end of its parenthetical); "The conductor matches your unit to its
baseline by that id, so even if you reword or truncate ..." (MARKERS: "matches" becomes "pairs your
unit with its baseline"); "A wrong or missing `criterion_id` is what makes your unit run as an EXTRA
unit on top of the baseline (duplicated work)."; "Still copy the criterion text into `criterion`
(verbatim is safest)."; "Several units echoing the SAME id (a real split) all run and replace the one
baseline."; "Propose each refinement by calling the rigger_emit tool with type \"UnitProposed\" and
data:" with its JSON shape (the cadence clause "the moment you decide it" cut); "A unit whose
`criterion_id` matches none of the ids below still runs, but as a genuinely-new sub-unit that the
conductor flags as unmatched." (its clause "- so only omit the id when you truly intend a new
sub-unit" cut and moved; FRAMING of
`conductor.rs:12155`; MARKERS: "matches none of the ids below" becomes "is none of the ids below");
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
line becomes "  PATTERN: {shape}. Record DecisionMade `{audit}` for it." and `ReviewRound::block`'s
"  PATTERN: {shape}. The implementer's audit record is DecisionMade `{audit}`." Two sentences land
verbatim in the `working-discipline` built-in's `## Hand over a tree the gates only confirm` section as
one added bullet: "- **A REQUIRED pattern is fixed everywhere.** Audit the whole tree, fix every site,
and record the DecisionMade your prompt names, listing every site checked. A reviewer checks it names
every site." (the slot `{audit}` replaced by "the DecisionMade your prompt names"; the reviewer
sentence is `ReviewRound::block`'s "check it names every site" with its subject named). No code check
is added.

**THE TEST AUTHOR IS DECLARED, decided here (criterion 10).** `Defaults` gains `test_author: String`,
default empty. `spawn_sdet_author` reads `self.cfg.agents.get(&cfg.workflow.defaults.test_author)`
when the key is non-empty and returns `Ok(())` (no spawn, no budget slot) when it is empty, whatever
agents `.rigger/agents` defines. The spawn id keeps the `sdet-author` role token
(`spawn_id(stage, ROLE_SDET_AUTHOR, attempt)`), so recorded ids, session resumption
(`continue_prior_session`) and the cache choice at :4507 are unchanged; only the agent definition is
the declared one. `Config::validate` refuses a non-empty `test_author` naming no defined agent,
naming the key and the id, as it refuses a stage agent. Criterion 10 owns every existing test whose
fixture relies on the fixed-id convention (the sdet tests at `conductor.rs:30253`, :37909, :44739,
:44846, :45024, :45115, :45208, :45314, :45405 at 3cee73df's numbering), adding `test_author` to its
fixture. The doc of `spawn_sdet_author` (:5185-5207, "The fixed `sdet-author` id is a convention ...
no `workflow.yml` change") is rewritten to the key. `SCAFFOLD_WORKFLOW` declares none (SCAFFOLD
DEFAULTS); this repository's workflow declares `test_author: sdet-author` (OPERATOR HAND-OFF).

**THE WORKFLOW SCRIPT NAMES NO TOOLCHAIN, decided here (criterion 11).** `WaveItem` (`spawn.rs:682`)
gains `build_cache_env: String`, always on the wire and non-null on every wave item (it is the
constant, decoupled from the cache path), its value the one constant `spawn::BUILD_CACHE_ENV`, which
criterion 11 adds (it does not exist at HEAD) and makes `RunCtx::spawn_env` (`conductor.rs:8464`)
and both gate-runner sites (`ExecRunner::run`, `crates/rigger-gates-shell/src/gate.rs:608`, :656)
read, value unchanged, the variable's name having one home; the
script's STEP schema (`rigger.js:194`, :202) lists it as required, `'string'`. The wire field `WaveItem::cargo_target_dir` (`spawn.rs:728`) keeps its name as CONTRACT: it is
the wire's field name, read by `${req.cargo_target_dir}` interpolations, never prompt text (Global
constraints). The constant's value is the toolchain coupling spec
120 leaves out of scope; the script's text names none. The rewordings: `workdir` (:624) "Do all your
file edits, your build tool's runs, and any git commit inside your isolated worktree ..."; BUILD
LOCATION (:659-661) "every command your build tool runs inside your worktree (anything that builds or
tests) MUST run with" then "  export ${req.build_cache_env}='${req.cargo_target_dir}'" then "set
first, in the same shell. That directory is the build cache the conductor chose for this spawn - ..."
(the rest as at HEAD, except its last sentence, which becomes "A build cache inside the worktree itself
is a defect that fills the disk - never create one."); SCRATCH POLICY (:673) keeps "NEVER under /tmp or your own session scratchpad"
and the reaper sentence, drops the parenthetical "(those are on the operator's small OS partition, and
a single cargo target or `rigger setup` shim install there fills the disk)", and its next sentence
reads "For any build you run outside your assigned worktree, export your build cache variable
(`${req.build_cache_env}`) to a subdir of that same printed path first - never the shared build
cache.", rendered for every spawn as at HEAD; its reaper
sentence becomes "It is reaped once your result records - do not store anything durable there." (the
rest of the sentence verbatim; criterion 11 owns the rewording, since "the moment" is a phrase-list
entry). The step courier (:892-894) runs `cd ${REPO} && rigger step --spec ...` with no cache prefix,
drops the sentence "(the CARGO_TARGET_DIR prefix makes every gate share one build cache ... - keep it
exactly as written)", and its timeout clause reads "a step integrates ready units by running their
gates inline and can take 20+ minutes ...". The prefix goes because the binary now sets the variable
on every branch a production caller reaches: `rigger step` pins every gate and regenerate build that has no per-unit cache onto
the shared cache it resolves from config (`shared_build_cache_paths`, `conductor.rs:8508`, its callers
:8619, :9178, :10309), and `ExecRunner::run` sets the variable from `build_cache_dir` when its guard
holds or was never requested, while its degraded branch (a guard requested whose probe failed,
`crates/rigger-gates-shell/src/gate.rs:651-656`) today withholds it and falls back to the ambient
variable, which only the courier's prefix fills (`${REPO}/.rigger/tmp/cargo-target`, outside the
scratch root, whose default is the cache home). The failure policy rides with criterion 11 because dropping the courier prefix leaves the degraded
branch no ambient fallback, so the unit that drops the prefix must decide that branch. A failed guard
probe (a guard requested whose probe failed) is an INFRASTRUCTURE FAULT, not a gate failure:
`ExecRunner::run` returns it as a distinct runner return, an error variant of the gate runner's
result (`Runner::run`, `crates/rigger-gates-shell/src/gate.rs:69`, returning `Result<GateResult,
ProbeFault>`, the fault carrying the probe error: the guard path and the OS error), never a
`GateResult` with a `pass` value, and runs and builds nothing anywhere: not into the gated tree, the shared cache (which spec 77 criterion
5's round-3 fix withheld from an unguarded build on purpose, `gate.rs:634-654`) or the ambient
directory. `run_gates_at` (`conductor.rs:8555`) propagates the fault before building any
`GateOutcome`, so it bypasses the F3 ordinal rerun (`conductor.rs:8551-8553`); verdicts already
recorded in the same pass stay and replay on the resumed step. The step then halts through the existing halt channel: `RunCtx::halt_reason`
(`conductor.rs:3960`), which the printed `Step.halted` carries to the driver and the operator, gains
that fault as an in-process halt reason set the way the budget breaker sets `budget_halted`
(`conductor.rs:3925`, the precedent, as hung liveness is for `Step.halted` in `src/cli/run.rs:879-882`);
no remediation attempt is charged, no `UnitFailed` and no `PriorFailure` evidence is written, and no
gate verdict is recorded. A gate run that requests no guard and names no directory inherits the
ambient environment; no production caller does this (every conductor call hands `build_cache_dir`
whenever `target_dir` is empty): the accepted residue, its behaviour unchanged, and criterion 11 owns
no existing test for that case. So the courier passes no cache prefix. Criterion 11 owns the branch,
the `ProbeFault` return, its propagation, the halt reason and their test: a forced guard-probe
failure returns the distinct return with the probe error, builds no `GateOutcome`, runs no F3 rerun,
records no gate verdict while keeping the pass's earlier verdicts, halts the step naming the error and
leaves no build directory behind. The script's comment lines are not prompt
text and stay. Criterion 11 edits the pins at `src/cli/mod.rs:5437-5450` that quote the old text.

**THE SCRATCH TOOL NAMES NO TOOLCHAIN, decided here (criterion 12).** The `rigger_scratch`
description (`mcpserver.rs:762`) reads "Your own rigger-assigned scratch container - the one directory
every probe repo, verification worktree, test build, or build cache you create must live under, never
/tmp or a session scratchpad. Reaped once your result records." ("Reaped the moment your result
records." reworded, since "the moment" is a phrase-list entry; criterion 12 owns the rewording).

**THE PLANNING SKILL CARRIES THE PROJECT'S CONSTRAINTS, decided here (criterion 13).** The repository
skill home is the operator overlay, not a forked skill: `DocsOverlay` (`src/cli/mod.rs:4599`)
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
(`write_docs`, `src/cli/mod.rs:5031`), so the committed source carries the generic step and the
installed copy carries this repository's list; the unit re-renders the committed source with the
tree's binary. Criterion 13 owns every existing test reading `PLANNING_A_SPEC_BODY` as a static body
(`docs.rs:1837` among them) and the `spec-preflight` test doc's "like `planning-a-spec`'s, ignores
`ctx`" (:2389).

**SKILL SHADOWING, decided here (criterion 14).** `install_skills` (`src/cli/mod.rs:4649`) checks, for
each registry entry, `<root>/.rigger/skills/<name>/SKILL.md`, `<name>` equal to the entry's name byte
for byte (case-sensitive, never trimmed). Present: that file's bytes are installed verbatim at
`.claude/skills/<name>/SKILL.md` (no overlay, no stamped prohibition: the project's copy is the
definition), written only when the installed file differs, and the outcome is `ProjectCopy` on every
setup while the shadow is in place (a state, not a write). `install_skills` returns `SkillInstall {
outcomes: Vec<(&'static str, InstallOutcome)>, strays: Vec<String> }`, `InstallOutcome` gaining the
`ProjectCopy` variant and `strays` carrying each project skill directory that shadows no shipped skill. Absent: the shipped render installs as at HEAD. A directory under
`.rigger/skills/` naming no registry entry (a near name included) shadows nothing, installs nothing,
and is named in `rigger setup`'s report, from `strays`, as shadowing no shipped skill. `rigger docs`, the docs-drift
gate and the pre-commit block read only the committed `skills/` sources and the shipped render, never
`.rigger/skills/`, so a shadow is never drift. An installed skill an operator edited by hand with no
project copy is rewritten as at HEAD; keeping an edit means copying it under `.rigger/skills/` first.

**THE IMPLEMENTER COMES WITH THE GATE SET, decided here (criterion 15).** `SetSpec` (`setup.rs:1628`)
gains a required `implementer: String`: a persona in markdown-with-frontmatter form, its `id` read
from the frontmatter. `scaffold/rust/set.yml` carries HEAD's `rust-engineer` seed text verbatim under
it. `SCAFFOLD_AGENTS`' `rust-engineer.md` entry is replaced by `implementer.md` (id `implementer`),
the HEAD seed's text with "in idiomatic Rust" cut, the language-neutral default. `SCAFFOLD_WORKFLOW`'s
two `agent: rust-engineer` lines become `agent: @IMPLEMENTER@`, which `scaffold_workflow` fills with the
matched set's implementer id or `implementer`. `init_project` adds the set's persona, as
`<id>.md`, to the seed candidates and writes every candidate, the `implementer` seed included, under
the same referenced-ids filter as the other seeds (`setup.rs:196-211`, `write_if_absent` unchanged): a
matched set's workflow references the set's id, so `implementer.md` is written only when no set
matched. `parse_template_set` refuses, naming the set, an implementer persona that does
not parse and one whose frontmatter `id` equals any `SCAFFOLD_AGENTS` id. The unwired `mutation.sh` and its `set.yml` comment stay: a project gate form. The
`SCAFFOLD_AGENTS` doc ("The ids match this project's own canonical personas (planner, rust-engineer,
...)", "implementer ... deliberately do NOT appear") and `authoring-loops.md`'s checklist step 3 ("your
language's engineer instead of `rust-engineer`") are rewritten to the set's implementer. Criterion 15
owns the seed-set test at `setup.rs:2743-2760`: its canonical set names `implementer` in place of
`rust-engineer` for the no-set render, and its stray list drops `implementer`, which is now a real
seed rather than the placeholder persona spec 05 removed.

**SCAFFOLD DEFAULTS, decided here.** The scaffold reproduces HEAD's behaviour for a fresh project
where HEAD gave every project one: `grep_guard: true` (HEAD guarded every project) and
`max_unit_added_lines: 3000` (HEAD capped every project); no `test_author` (HEAD spawned one only for a
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
crate only the texts it renders itself (the critique prompt's framing and `CRITIQUE_OUTPUT_CONTRACT`,
`review.rs`); `rigger-conductor` the conductor's and the canary's texts; `rigger-driver` the cli
host's; `rigger-dash` the MCP tool descriptions and their `inputSchema` property descriptions
(`mcpserver.rs:757-773`); the binary crate (`src/`) the critic
spawn's framing (`src/cli/critique.rs`), `GREP_GUARD_MESSAGE`, the write-guard deny reason and the
SessionStart `rigger prime` output's fixed texts (its headers and labels, never the store data they
frame), whose renderer (`cmd_prime` and `instructions_in_force_line`, `src/cli/setup.rs:1410-1478`)
criterion 16 moves into a new `src/cli/prime.rs`, so `setup.rs` stays outside the sweep. No adapter text moves inward across a crate
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
critique prompt's kept framing and `CRITIQUE_OUTPUT_CONTRACT`, the canary's `review_header` and
`adjudicator_prompt` (`canary_store.rs:646-679`), `framed`, `rigger prime`'s output, the MCP tool descriptions of all three
surfaces and their `inputSchema` property descriptions (moved out of the `json!` literals in
`tool_list`, `mcpserver.rs:750-776`) and
`GREP_GUARD_MESSAGE` and the `rigger guard-write` deny reason (`guard_write_deny_outside_first_root`,
`src/cli/guard.rs:405-410`, CONTRACT: the reason the write guard reports). Every crate that renders one
already depends on `rigger-domain`. Each HIT ENUMERATION row names its owner; criterion 16 adds no
rewording; criterion 17 only asserts. A text moves only within its crate, into that crate's
`prompt_surface` module, which is its one home; the renderer reads the group. Criterion 16 re-paths
every reader, among them PR #117's helper and spec 120's criterion 12 and 13 tests, which read
`conductor::REVIEWER_DISCIPLINE` and then read `rigger_conductor::prompt_surface::REVIEWER_DISCIPLINE`. The two
continuation imperatives ("Finish and report; do not start over.", "Fix exactly these - do not start
over:"), `CRITIQUE_OUTPUT_CONTRACT`'s remedy field and the fan-out section's tier names and cap stay
FRAMING (the audit's borderline kept). `HELPER_AGENTS`, the scaffold seeds, the skills and the
built-ins are the consumer's surfaces, never registry entries. The concatenation test: for each
surface, the renderer run over a fixture whose every string slot value is the literal `{slot}` (one
row where the renderer loops) equals the surface's groups for that fixture concatenated in registry
order (whitespace-normalized); a slot that is not a string (a number, a count) takes a fixed fixture
value named in the test, and the expected text is the groups filled through `fill` with that value's
rendering, then concatenated; a surface with mutually exclusive branches (a PASS or a FAIL row, an empty
or a joined list, a stage with or without a criterion) gets one fixture per branch, each naming the
groups that branch renders, so every group is rendered by some fixture. For `tool_list` the test
collects every `description` string of the advertised JSON in document order and compares that
list's concatenation with the dash crate's groups. For `rigger prime`'s output the test runs the hook's form (no spec argument; the spec-argument
branch's `spec_lint_next_step` line is operator output outside the registry) and collects the
fixed texts in render order over a fixture store, the store data (decision ids and summaries, the
operator file count) replaced by `{slot}` values. The binary crate's `prompt_surface` module lives in its LIB target (`src/lib.rs` gains `pub mod
prompt_surface`), which `src/cli` reads; it also exposes `pub fn all_groups() -> Vec<&'static Group>`,
collecting every crate's `prompt_groups()` plus its own, the one enumeration. PR #117's
`every_shipped_agent_text()` (a bin-test helper) reads `all_groups()`, labelling each group `REGISTRY
<crate>::<group>`; the sweep (`tests/prompt_surface_sweep.rs`) and criterion 17's tests read
`all_groups()` directly, never the bin helper. Each crate's `prompt_surface` module is ungated pure
consts within its crate; `all_groups()` carries the lib's existing store-gate predicate
(`#[cfg(any(feature = "store", not(feature = "core")))]`, `src/lib.rs:19`) for the outer crates'
groups, so the `core` lane compiles the domain's and the binary crate's modules alone, and the sweep,
the concatenation tests and criterion 17's test run on every lane the store predicate admits (default
and no-default), never `core`, gated as the lib's outer modules are.

**THE CLASS TABLE, decided here (criterion 16).** Every surface's groups with the class each group's
doc comment opens with; a surface with two classes has one row per class.

| Crate | Surface | Groups | Class |
| --- | --- | --- | --- |
| conductor | `EMIT_PROTOCOL` | the `rigger_emit` call and its JSON shape | CONTRACT |
| conductor | `PLAN_PROTOCOL` | the planner's role, the baseline, the refine sentence, the pairing, extra-unit, real-split, unmatched-id and fan-out-gates facts, the criteria lead-in | FRAMING |
| conductor | `PLAN_PROTOCOL` | the split's OWNS sentence, the stable-id, echo and copy-the-text sentences, the `UnitProposed` call and its JSON shape | CONTRACT |
| conductor | `unit_size_cap` | the declared cap sentence | FRAMING |
| conductor | `REQUIRED_PROTOCOL` | the verdict line's REQUIRED list and its shape | CONTRACT |
| conductor | `REVIEWER_DISCIPLINE` | what the harness enforces or provides (spec 120's three sentences) | FRAMING |
| conductor | `review_protocol` | the `rigger_emit` finding call and its JSON shape | CONTRACT |
| conductor | `DECISIONS_HEADER`, `CODE_NEIGHBORHOOD_HEADER`, `DESIGN_INTENT_HEADER`, the lessons and findings headers, the elision notes | each header and note | FRAMING |
| conductor | `graph_around_recovery` | the recovery command | CONTRACT |
| conductor | `task_block` | the unit's criterion | FRAMING |
| conductor | `PriorFailure::block` | the failure facts | FRAMING |
| conductor | `PriorFailure::block` | the PATTERN line's record id | CONTRACT |
| conductor | `ReviewRound::block` | the round facts | FRAMING |
| conductor | `ReviewRound::block` | the PATTERN line's record id | CONTRACT |
| conductor | `gate_evidence_block` | its rows and framing sentence | FRAMING |
| conductor | `conflict_resolution_prompt`, `later_attempt_block` | the conflict and delta facts, the two continuation imperatives | FRAMING |
| conductor | `DOC_ONLY_ROUND` | the fact | FRAMING |
| conductor | `write_peers_pointer` | the retrieval facts | FRAMING |
| conductor | `write_lookup_pointer` | the lead-in "The knowledge graph answers lookups through three verbs:" | FRAMING |
| conductor | `write_lookup_pointer` | the three verbs and their jobs | CONTRACT |
| conductor | the DAG critique prompt, `re_plan` | the DAG, the cap framing, the critique's findings | FRAMING |
| conductor | the DAG critique prompt | the verdict and cause contracts | CONTRACT |
| conductor | the canary's `review_header`, `adjudicator_prompt` | the item and its review facts | FRAMING |
| domain | the critique prompt | its kept framing | FRAMING |
| domain | `CRITIQUE_OUTPUT_CONTRACT` | the finding lines, the remedy field included | CONTRACT |
| driver | `framed` | the task frame | FRAMING |
| driver | `framed` | the progress verb line | CONTRACT |
| dash | the MCP tool descriptions and `inputSchema` property descriptions | each tool and argument | CONTRACT |
| binary | the critic spawn's framing (`src/cli/critique.rs`) | the spec and its critique facts | FRAMING |
| binary | `GREP_GUARD_MESSAGE`, the `rigger guard-write` deny reason | each denial | CONTRACT |
| binary | `rigger prime`'s output (`src/cli/prime.rs`) | the instructions-in-force line, the decisions header, the no-store and none-yet lines | FRAMING |

**THE PHRASE AND MARKERS TESTS, decided here (criterion 17).** One test, in `src/cli/setup.rs`'s
tests beside spec 120's MARKERS function (a test-module function the `tests/` directory cannot reach),
iterates `rigger::prompt_surface::all_groups()` directly and the scripts' contract and framing region
entries (criterion 18, which 17 needs), read through the same region reader the sweep uses, and fails on any phrase of
a list kept in the test, matched as a lowercase substring: "the moment", "never batch", "do not
batch", "keep it flowing", "between your actions", "between your own actions", "do not contradict",
"supersede explicitly", "default to skepticism", "reject only", "not by grepping", "never a new test",
"audit the whole tree", "do not write code", "scope creep". A second assertion runs spec 120's
MARKERS rule (the function its criterion 12 test defines, called, never copied) over the same texts,
a script literal's `${...}` interpolations removed first. A negative fixture holds HEAD's
`EMIT_PROTOCOL` text in the test and asserts the phrase check flags it. The test never reads
`BUILTIN`, the personas or the skills: prescription lives there by design.

**THE SOURCE SWEEP, decided here (criterion 18).** A test walks every `.rs` file under `crates/` and
`src/`, excluding every `tests/` directory (the repository root's and each `crates/*/tests/`) and every
file declared through `#[cfg(test)] mod <name>;`. It cuts each file only at a `#[cfg(test)]` line
directly followed by a `mod <name> {` line (the file's test module, the same cut PR #117's
production-line pin uses); every other `#[cfg(test)]`-gated item and every `#[cfg(any(test, ...))]`
item before that cut is scanned as production. The PROMPT-BEARING files are derived by token, never
listed by hand: every such file whose production text spells `SpawnOpts`, `SpawnHost` or
`prompt_surface`, so a new file touching the spawn surface or the registry is swept automatically. The token marks every renderer, since each crate's module carries the name `prompt_surface`. The
test tokenizes their string literals (escaped and raw, line continuations joined) and selects every
literal of 40 or more characters containing a space, by shape alone. Each selected literal outside a
crate's `prompt_surface` module is in a registry group (its exact text, or contained in a registered
group) or in an allow-list kept in the test, keyed by file, with two named categories: "operator
message" (an error, a status line, a help text, a log line the operator reads) and "tool output" (a
tool result or error an agent reads back, such as the MCP tool results and errors in `mcpserver.rs`),
one prefix entry per literal. Tool results and errors are OUTPUT, data an agent reads back, not text
placed before it as instruction, so they are outside the prompt surface and the registry; criterion
18 owns every entry of both categories, among them `hooks.rs`'s error-format literal
("helper agent {file}: frontmatter line {line:?}") and `spec_lint_next_step` (`src/cli/mod.rs:5071`,
which stays there as operator output) as operator messages. `setup.rs` and `docs.rs` spell none of the three tokens and no
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
literals, of any length, joined in source order with a single space, interpolations written `{}`
(a template literal nested inside an interpolation is a literal of its own, joined into the entry in
source order and selected by the 40-character rule),
labelled `SCRIPT <file>:<line>` by the region's opening line; criterion 17's two assertions run over
that joined entry.

**OPERATOR HAND-OFF, decided here.** No unit edits `.rigger/workflow.yml`: the run's driver is the
installed binary, whose `Defaults` refuses an unknown key (`deny_unknown_fields`, `config.rs:522`),
so a unit landing a new key would halt every later step. After the run's PR merges, the operator
refreshes the binary, then adds `grep_guard: true`, `max_unit_added_lines: 3000` and `test_author:
sdet-author` under this repository's `defaults`, then runs `rigger setup` (which with the key
declared keeps the guard). `.rigger/docs-overlay.yml` (criterion 13) binds the same way: a pre-121
binary's `DocsOverlay` refuses the unknown `global_constraints` field (`deny_unknown_fields`), so no
`rigger setup` runs with a pre-121 binary from criterion 13's integration on `rigger-run` until the
merge and the refresh; the refresh comes first. Persona edits (criteria 6, 7, 8) change files `definition_hash` folds
(`src/cli/mod.rs:974`); each such unit's integration is a definition drift the operator resumes
with `--rebase-definition`, as for spec 120's criterion 15. This repository tracks its helper copies
(`.claude/agents/lookup.md`, `verify.md`), so their fold changes its definition hash: its paused run
relaunches once with `--rebase-definition` after the refresh.

**CONSTRAINTS WALK, decided here.**

| Mechanism | Corner | Decision |
| --- | --- | --- |
| new workflow keys | empty | absent: no guard, no cap sentence, no test-author spawn |
| new workflow keys | existing data | an existing workflow.yml without them loads and behaves as declared, not as before: an upgraded project loses its guard (setup removes rigger's entry), its cap and its sdet-author spawn until it declares them; accepted and named in the handbook's upgrade note (criterion 2's, 7's and 10's lines in `authoring-loops.md`) |
| new workflow keys | an older binary reading a new workflow | refused at load (`deny_unknown_fields`); OPERATOR HAND-OFF orders the refresh first |
| new workflow keys | reverted, DROPPED | a key removed between steps is a definition drift, as any workflow edit; rebased, the next spawn reads the new value |
| new workflow keys | concurrent, crash-resume, cold start | read from the one loaded `Config` per process, never cached across processes |
| `test_author` | repeated | one key, one agent; the role token stays `sdet-author`, so a renamed agent resumes the token's recorded session, as a renamed adjudicator does at HEAD; accepted |
| `test_author` | existing store | recorded ids unchanged; a store whose run spawned the conventional agent replays by id |
| `grep_guard` | existing data | a settings.json carrying the guard and a foreign PreToolUse hook, in one block or two: setup removes only inner hooks whose command is `GREP_GUARD_COMMAND`, any matcher, dropping a block only once its inner array is empty |
| `grep_guard` | repeated | setup twice with the key declared writes once; absent twice removes once |
| `grep_guard` | empty | no `settings.json` and the key off: nothing read or written, no `.claude/` directory created, `AlreadyCurrent` |
| `grep_guard` | drifted matcher | an installed guard hook whose matcher differs from `GREP_GUARD_MATCHER`: an install with the key true replaces it, so exactly one entry with the current matcher remains |
| `grep_guard` | reverted | the key flipped false after a setup: the installed hook denies until the next `rigger setup`; accepted |
| spawn host | concurrent, crash-resume, cold start | `resolve_spawn_host` reads the loaded defaults and the root once per step, critic run or canary run; every process resolves again |
| helper copy | empty | no `.claude/agents/` file: the embedded text |
| helper copy | DROPPED | an operator copy deleted from the operator's checkout mid-run: the copy present at the pin folded into `definition_hash` and is gone, so the next step halts on drift; resumed with `--rebase-definition`, the next spawn reads the embedded text, whatever a unit worktree still carries |
| helper copy | existing pinned run | a run pinned before the refresh in a project with a copy present resumes once with `--rebase-definition`; without a copy it resumes unchanged |
| helper copy | edited mid-run | a definition drift, halted and resumed with `--rebase-definition`, as a persona edit is |
| helper copy | existing data | an operator copy written by an older `rigger init` and never edited equals the embedded text at that version; a newer binary keeps passing the old copy; accepted (the copy is the definition) |
| helper copy | malformed | the spawn fails naming the file; `rigger validate` names it first |
| skill shadow | existing data | a hand-edited installed skill with no project copy is rewritten as at HEAD |
| skill shadow | near name, repeated | a near name shadows nothing; a second setup writes nothing |
| skill shadow | DROPPED | a project copy deleted: the next setup installs the shipped render |
| overlay field | empty | absent or empty list: the generic step only |
| scaffold implementer | existing data | an existing project's workflow and personas are write-once and stay; only a fresh `rigger init` gets the new ids |
| gate cache | degraded guard | an infrastructure fault, a runner error `run_gates_at` propagates before any `GateOutcome`, bypassing the F3 rerun: the step halts naming the probe error, no attempt charged, no gate verdict or `PriorFailure` written, the pass's earlier verdicts kept and replayed on resume; nothing builds into the gated tree, the shared cache or the ambient directory |
| gate cache | no guard, no directory | a gate run that requests no guard and names no directory inherits the ambient environment; no production caller does this; accepted residue |
| `build_cache_env` | empty | never null: a wave item with no per-unit cache still carries the name, so SCRATCH POLICY's export sentence renders for every spawn, as at HEAD |
| scope sentence | DROPPED | `unit_size_cap`'s second sentence leaves the DAG prompt and lands in the three critique and planning personas and their scaffold templates in the same commit |
| scope sentence | `max_unit_added_lines` absent | the moved scope sentence stays in the three personas and is inert: it qualifies a cap only when the prompt states one; accepted |
| moved sentences | existing consumer personas | ACCEPTED: write-once personas are never rewritten; the handbook upgrade note names each moved sentence and its persona home |
| registry | empty surface | a renderer whose optional parts are all absent renders a subset of its groups; the concatenation fixture covers every branch present |
| sweep | a new literal | fails until registered or allow-listed |
| moved sentences | switched off | a consumer switching `working-discipline` or `communication` off removes the moved sentences with the rest: intended |
| moved sentences | MARKERS | each moved sentence carries no spec-120 marker, so spec 120's criterion 12 test stays green |

## Notes (non-criteria)

INTERIM SENTENCES: a doc or handbook sentence a later criterion makes false is left by an earlier
unit as written, listed in its hand-over, and raises no finding. Assignments: `write_lookup_pointer`'s
and `write_peers_pointer`'s docs to 1; `GREP_GUARD_MATCHER`'s doc, `install_session_settings`'s doc,
`spawn_config_args`' doc bullet on `--settings` and `tools-and-context.md:107` to 2; `HELPER_AGENTS`'
doc ("cannot drift from them"), the `--agents` bullet of `spawn_config_args`' doc and
`tools-and-context.md:109` to 3; `EMIT_PROTOCOL`'s and `DECISIONS_HEADER`'s docs to 4; the shim's and
the script's `progressNote` comments and `framed`'s doc to 5; `DOC_ONLY_ROUND`'s doc to 6;
`MAX_UNIT_ADDED_LINES`' doc (deleted) and `unit_size_cap`'s doc to 7; `PLAN_PROTOCOL`'s doc ("a
proposed unit that maps to NO criterion is scope creep and is refused") to 8; `ReviewRound`'s `audit`
field doc to 9; `spawn_sdet_author`'s doc to 10; the `buildLocation` comment (`rigger.js:653-657`) to
11; the `SCAFFOLD_AGENTS` and `SCAFFOLD_WORKFLOW` docs and `authoring-loops.md:144` to 15; the
handbook's upgrade note for the three keys (a new paragraph after `authoring-loops.md`'s "Upgrading an
older workflow" paragraph, one sentence per key) to 2, 7 and 10 in that order, each adding its own
sentence; the same note's persona lines (one per moving criterion, naming each sentence it moves and
that sentence's persona home, since an existing project's write-once personas never gain it) to 6,
7 and 8, each adding its own line; each item of the open-exceptions list in `authoring-loops.md`'s `## What rigger prescribes`
to the criterion EXCEPTIONS LIST names (1, 4, 5, 6, 7, 8, 9, 11, 12), and the emptied list with its
lead-in sentence to 17.

OUT OF SCOPE: the build-cache variable's value (the constant itself is criterion 11's) and the
grounder's Rust-only extraction, as spec 120 decides; the SessionStart hook mechanism (its `rigger prime` output's fixed texts are criterion 16's registry
surfaces); the status line (`console::statusline`, `rigger status --line`), the editor's display,
read by the operator and never by a spawn's model, so outside the Goal's claim;
the liveness heartbeat, the self-report and the courier relay contracts; every other shipped skill's
body (each becomes shadowable by criterion 14); `HELPER_AGENTS`' text; the personas' other content;
the `mutation.sh` template; the audit's borderline items, kept FRAMING; everything spec 120 owns
(its OUT OF SCOPE names the split).

LANES (criterion 19): the `core` lane compiles each ungated `prompt_surface` module of the domain and
the binary crate alone; `all_groups()`, the sweep, the concatenation tests and criterion 17's test run on
every lane the store predicate admits (default and no-default), never `core` (THE REGISTRY).

UNIT ORDER (the needs chain): 1, 2, 3, 4, 5, 6, 8, 7, 9, 10, 11, 12, 13, 14, 15, then 16, then 18,
then 17, then 19. The moves 1 to 15 are independent except two named needs: 3 needs 2, and 7 needs 8
(shared files otherwise serialize through the blast radius). 16 needs 1 to 15; 18 needs 16; 17 needs
18; 19 needs all.

LANDING-ORDER SIMULATION (every ordered pair sharing a surface):

- 1 and 9 (the `working-discipline` body): different sections, each adds its own sentences -> yes
  either order.
- 4 and 5 (`shim.mjs`): 4 edits `renderPeers`, 5 the progress note -> yes either order.
- 4 and 8 (`PLAN_PROTOCOL`): the planner's cadence clause is 8's alone -> yes.
- 8 then 7 (`PLAN_PROTOCOL`, the planner personas): 8 moves its sentences and rewords the two MARKERS
  hits; 7 then renders the cap slot and places its moved sentence directly after 8's split sentence
  -> yes. 7 first: the split sentence it follows does not exist -> fixed by order (7 needs 8).
- 2 then 3 (`SpawnHost`, `SpawnOpts`, `Deps::spawn_host`, `spawn_config_args`): 2 lands `SpawnHost`
  with `grep_guard`, `resolve_spawn_host(defaults)`, `Deps::spawn_host`, the accessor, the three
  resolution points and every construction; 3 adds the `repo_root` argument, `helpers`, its fold and
  its readers -> yes. 3
  first: no `SpawnHost` to extend -> fixed by order (3 needs 2).
- 6 and 10 (`spawn_sdet_author`): 6 first: its test spawns by the convention; 10 then adds
  `test_author` to that fixture, which it owns -> yes. 10 first: 6's test declares the key -> yes.
- 5 and 11 (`rigger.js`): 5 the progress note, 11 the other worker and courier texts -> yes.
- 13 and 14 (`install_skills`): 13's installed-copy test needs only the overlay; 14 installs a shadow
  verbatim, never overlaid -> yes either order.
- 2, 7, 15 (`SCAFFOLD_WORKFLOW`): each adds its own key line or placeholder -> yes.
- 16 after 1 to 12: 16 relocates final texts; before them it would relocate texts later criteria
  edit -> fixed by order.
- 18 without 16: every selected literal would be unregistered -> fixed by order (18 needs 16).
- 17 without 18: no script regions to iterate -> fixed by order (17 needs 18, which needs 16, which
  needs the moves, so no phrase hit remains).

TEST PLACEMENT:

| Criterion | Test homes |
| --- | --- |
| 1 | `conductor.rs` tests (the pointer's text); `src/cli/setup.rs` tests through the shipped-text helper (the built-in's section) |
| 2 | `crates/rigger-driver/src/hooks.rs` tests (merge with and without the flag); `src/cli/setup.rs` tests (`install_lookup_hook` install, remove, foreign hook kept, a drifted matcher replaced, no `.claude/` created with the key off); `crates/rigger-config-files/src/config_store.rs` (the key parses) |
| 3 | `hooks.rs` tests over a temp root with and without an operator copy, a malformed copy failing `resolve_spawn_host`; `driver/mod.rs` tests (`--agents` and the pre-approval from one copy); `src/cli/mod.rs` tests (`definition_hash` changes with an operator copy) |
| 4 | `conductor.rs` tests (`EMIT_PROTOCOL`, `review_protocol`, `DECISIONS_HEADER`); `src/cli/mod.rs` tests over `SHIM_MJS` |
| 5 | `driver/cli.rs`, `mcpserver.rs` tests; `src/cli/mod.rs` tests over `RIGGER_WORKFLOW` and `SHIM_MJS`; `instructions.rs` tests (the built-in's bullet) |
| 6 | `conductor.rs` tests (the doc-only sdet prompt); `src/cli/setup.rs` tests over the committed persona |
| 7 | `conductor.rs` tests (both prompts with the key declared and absent); `config_store.rs` (the key); `src/cli/setup.rs` tests (personas, seeds, `SCAFFOLD_WORKFLOW`) |
| 8 | `conductor.rs` tests (`PLAN_PROTOCOL`); `src/cli/setup.rs` tests (persona and seed) |
| 9 | `conductor.rs` tests (both PATTERN lines); `src/cli/setup.rs` tests (the built-in's bullet) |
| 10 | `conductor.rs` tests (declared, absent with `sdet-author` defined, an undefined id refused); `config_store.rs` tests (the refusal) |
| 11 | `spawn.rs` tests (the `build_cache_env` wire field); `src/cli/mod.rs` tests over `RIGGER_WORKFLOW`; `gate.rs` tests (a forced guard-probe failure returns the `ProbeFault` error with the probe error and leaves no build directory behind); `conductor.rs` tests (that return halts the step naming the error, with no `GateOutcome`, no F3 rerun, no gate verdict, no charged attempt and no `PriorFailure`, earlier verdicts of the pass kept) |
| 12 | `mcpserver.rs` tests |
| 13 | `docs.rs` tests (the generic step); `src/cli/mod.rs` tests (`install_skills` with an overlay list) |
| 14 | `src/cli/mod.rs` tests over a temp root (a shadow, a near name, a deleted shadow); a periphery test running the built binary's `setup` |
| 15 | `src/cli/setup.rs` tests (`init_project` with and without `Cargo.toml`) |
| 16 | each crate's `prompt_surface` tests (no duplicate (surface, name)); a lib test that `all_groups()` holds every crate's `prompt_groups()`; a concatenation test beside each renderer (`conductor.rs`, `review.rs`, `canary_store.rs`, `driver/cli.rs`, `mcpserver.rs`, `src/cli/mod.rs`, `src/cli/prime.rs`, `src/cli/critique.rs`, `src/cli/guard.rs`) |
| 17 | `src/cli/setup.rs` tests reading `rigger::prompt_surface::all_groups()` and the region entries directly, beside spec 120's MARKERS function |
| 18 | `tests/prompt_surface_sweep.rs` over the source tree and the two script files, reading `rigger::prompt_surface::all_groups()` |
| 19 | the lanes script on the integrated tree |

The registry tests of criteria 16, 17 and 18 that read `all_groups()` run on every lane the store predicate
admits (default and no-default), never `core`, gated by the lib's store predicate as its outer modules are; the `core` lane compiles the domain's
and the binary crate's `prompt_surface` modules and their own tests alone.

ARTIFACT OWNERSHIP (owner = the criterion of the first production caller):

| Artifact | Owner |
| --- | --- |
| the pointer's framing paragraph, the Fan out section's two sentences and the cut of its "same rule" sentence | 1 |
| `Defaults.grep_guard`, the flag's reading in `resolve_spawn_host`, `carries_session_settings`' flag, the `cmd_setup` workflow parse, `session_hooks_advisory`'s guard clause and its stale-hook advice, the guard arms of `install_session_settings` and `install_lookup_hook` (the drifted-matcher replacement and the off-path's no-create read included), the scaffold's `grep_guard` line | 2 |
| `hooks::resolved_helpers`, the `repo_root` argument at the three resolution points, `SpawnHost.helpers` (the test literals included), its fold into `definition_hash` (each present copy), its readers `spawn_config_args` and `allowed_tools_args` with both `build_args` callers, the validate check of the copies | 3 |
| `SpawnHost` with `grep_guard`, `hooks::resolve_spawn_host(defaults)`, `Deps::spawn_host`, the `RunCtx` accessor, the three resolution points (`rigger step` before `enforce_definition_pin`, the critic, the canary), `SpawnOpts.host`, `SpawnOpts::new`, every construction site's edit, the no-`SpawnHost {` assertion | 2 |
| the cut cadence clauses of `EMIT_PROTOCOL`, `review_protocol`, `DECISIONS_HEADER`, `renderPeers` | 4 |
| the four hosts' progress texts, the `communication` built-in's progress bullet, the extended spec-120 criterion 14 body pin | 5 |
| `DOC_ONLY_ROUND`'s fact text, the sdet-author persona's section | 6 |
| `Defaults.max_unit_added_lines`, `unit_size_cap(cap)`, both prompts' cap sentence, the moved second sentence in the planner, adversary and adjudicator personas and their scaffold templates, the scaffold's cap line, the pins of `unit_size_cap` and `MAX_UNIT_ADDED_LINES` | 7 |
| `PLAN_PROTOCOL`'s guidance cut and MARKERS rewordings, the planner persona's and seed's moved sentences | 8 |
| both PATTERN lines, the hand-over section's bullet | 9 |
| `Defaults.test_author`, its refusal, the spawn's lookup, the edited sdet fixtures | 10 |
| `WaveItem::build_cache_env`, `spawn::BUILD_CACHE_ENV` and its three rewired readers (`RunCtx::spawn_env`, `gate.rs:608` and :656), the script's reworded texts, the courier command, the probe's `ProbeFault` return, its propagation, its halt reason and their tests | 11 |
| the `rigger_scratch` description | 12 |
| each open-exceptions item's strike | the criterion EXCEPTIONS LIST names |
| `DocsOverlay.global_constraints`, the `DocsContext` field, `planning-a-spec` as `Rendered`, `.rigger/docs-overlay.yml`, the re-rendered `skills/planning-a-spec/SKILL.md`, the edited static-body tests | 13 |
| the shadow rule in `install_skills` and its report lines | 14 |
| `SetSpec.implementer`, `scaffold/rust/set.yml`'s persona, the `implementer` seed, `@IMPLEMENTER@`, the edited seed-set test | 15 |
| `rigger_domain::prompt_surface` (`Group`, `fill`), the prime renderer's move into `src/cli/prime.rs` and its groups, THE CLASS TABLE's doc-comment class words, each crate's `prompt_surface` module with its texts and `prompt_groups()`, the lib's `pub mod prompt_surface` and `all_groups()`, every renderer's move onto it, the re-pathed readers, the helper's registry entries | 16 |
| the phrase list, the MARKERS call over the registry, the negative fixture, the removal of the emptied open-exceptions list and its lead-in | 17 |
| the sweep, the prompt-bearing derivation, its allow-list, the script regions (operator regions included), the region reader and their helper entries | 18 |
| the lanes | 19 |

## Global constraints

- Hyphens, never em or en dashes, ASCII only, in every added line.
- A line citation names the symbol at the tree it was read on (3cee73df, on `rigger-run`); the symbol
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
- [ ] a test proves THE TASK PROMPTS CARRY NO COMMUNICATION CADENCE: `EMIT_PROTOCOL`, `review_protocol`, `DECISIONS_HEADER` and the shim's `renderPeers` keep their shapes and nouns with every cadence clause Design names cut, the `communication` built-in stating it. This criterion OWNS those four texts and their pins; the progress cadence is criterion 5's, the planner's clause criterion 8's, NOT this one's.
- [ ] a test proves THE PROGRESS CADENCE HAS ONE HOME: the cli host's `framed`, the workflow script's and the shim's progress notes and the `rigger_progress` description name the verb and its arguments only, and the cadence wording Design names stands in the `communication` built-in. This criterion OWNS the four host texts, the built-in's bullet and their pins; other cadence is criterion 4's, NOT this one's.
- [ ] a test proves A DOC-ONLY ROUND STATES ONLY THE FACT: the sdet-author prompt on a round `is_documentation_only` classifies carries "This round changed documentation only." and no rule, and the rule's sentence stands verbatim in `.rigger/agents/sdet-author.md`. This criterion OWNS `DOC_ONLY_ROUND`, its doc, the persona's section and their pins; the test-author key is criterion 10's, NOT this one's.
- [ ] a test proves THE UNIT SIZE CAP IS DECLARED: `unit_size_cap` renders its first sentence from `defaults.max_unit_added_lines` in the DAG critique prompt and `PLAN_PROTOCOL` only when declared, neither carrying a cap sentence when absent, `MAX_UNIT_ADDED_LINES` gone and the second sentence in the planner and both critique personas. This criterion OWNS the key, the cap text and its pins; other planner guidance is criterion 8's, NOT this one's.
- [ ] a test proves THE PLANNER PROTOCOL CARRIES NO PLANNING POLICY: `PLAN_PROTOCOL` holds exactly the sentences Design keeps as framing, and every sentence Design moves (the split, discovery, scope-creep, cadence, no-code and omit-the-id sentences) stands verbatim in this repository's planner persona and the scaffold's planner seed. This criterion OWNS both lists and their pins; the cap sentence is criterion 7's, NOT this one's.
- [ ] a test proves THE REQUIRED PATTERN NAMES ONLY ITS RECORD: `PriorFailure::block` and `ReviewRound::block` give each REQUIRED pattern item its shape and its `audit_id` record id, and the audit and check sentences stand in the `working-discipline` built-in's hand-over section. This criterion OWNS both PATTERN lines, that bullet and their pins; the lookup sentences in that body are criterion 1's, NOT this one's.
- [ ] a test proves THE TEST AUTHOR IS DECLARED: `spawn_sdet_author` spawns the agent `defaults.test_author` names, under the `sdet-author` role token, and spawns nothing when the key is absent, whatever `.rigger/agents` defines; an undefined id is refused at load. This criterion OWNS the key, its refusal, the spawn's lookup and the edited fixtures; the doc-only rule is criterion 6's, NOT this one's.
- [ ] a test proves THE WORKFLOW SCRIPT NAMES NO TOOLCHAIN: `RIGGER_WORKFLOW`'s worker and courier texts name the cache only through `WaveItem::build_cache_env` and the wave's path, without the disk-layout sentence, courier prefix or "the moment", and a failed guard probe halts the step as an infrastructure fault naming its error. This criterion OWNS those texts, the wave field, the probe fault and their pins; `rigger_scratch` is criterion 12's, NOT this one's.
- [ ] a test proves THE SCRATCH TOOL NAMES NO TOOLCHAIN: the `rigger_scratch` description that `mcpserver.rs`'s `tool_list` advertises says "build cache" where it named the toolchain's cache variable and "Reaped once your result records.", keeping the reaped container's contract word for word otherwise. This criterion OWNS that description and its pins; the workflow script's texts are criterion 11's, NOT this one's.
- [ ] a test proves THE PLANNING SKILL CARRIES THE PROJECT'S CONSTRAINTS: `planning-a-spec` step 6 reads "Carry the project's own Global constraints" with neutral path examples, and the `global_constraints` list of `DocsOverlay` renders under it in the installed copy. This criterion OWNS the body's wording, the overlay field and this repository's overlay file; skill shadowing is criterion 14's, NOT this one's.
- [ ] a test proves A PROJECT SKILL SHADOWS THE SHIPPED ONE: `install_skills` installs `.rigger/skills/<name>/SKILL.md` verbatim in place of the shipped skill of that exact name, a near name shadowing nothing, while `rigger docs` and the drift gate stay on the shipped text. This criterion OWNS the shadow rule and its setup report lines; the skill bodies are criterion 13's, NOT this one's.
- [ ] a test proves THE IMPLEMENTER COMES WITH THE GATE SET: `init_project` with the Rust set writes its `implementer` persona and names that id in `SCAFFOLD_WORKFLOW`'s implement and checkin stages, and with no set writes and names a language-neutral `implementer`. This criterion OWNS `SetSpec.implementer`, the seed, the placeholder and their pins; the scaffold's keys are criteria 2 and 7's, NOT this one's.
- [ ] a test proves THE REGISTRY HOLDS EVERY PROMPT TEXT: each crate's `prompt_surface` module holds its crate's prompt texts as `Group`s whose doc comments open with the class Design's table assigns, every renderer formats through `fill`, and each surface's groups concatenate to its render over the placeholder fixture. This criterion OWNS the types, every crate's module and the concatenation tests; the phrase test is 17's, the sweep 18's, NOT this one's.
- [ ] a test proves NO REGISTERED TEXT PRESCRIBES: every group `prompt_surface::all_groups()` returns and every joined script region entry carries no phrase of the test's cadence and method list and no hit under spec 120's MARKERS rule, and a held pre-move `EMIT_PROTOCOL` fails it. This criterion OWNS the phrase list, both assertions and removing the emptied exceptions list; the registry is criterion 16's, NOT this one's.
- [ ] a test proves THE SOURCE SWEEP CLOSES THE REGISTRY: every production literal of 40 or more characters with a space in a prompt-bearing file (one spelling `SpawnOpts`, `SpawnHost` or `prompt_surface`) is registered or allow-listed, and each such literal of the two embedded scripts sits in a classed region. This criterion OWNS the sweep, the file derivation, the allow-list and the regions; the registry is criterion 16's, NOT this one's.
- [ ] `.rigger/gates/lanes.sh` runs green for both feature lanes on the finished tree, verified by `sh .rigger/gates/lanes.sh no-default` and `sh .rigger/gates/lanes.sh core` each exiting 0, with `cargo fmt --check` passing. This criterion OWNS only the lanes; every behaviour is criteria 1 to 18's, NOT this one's.
