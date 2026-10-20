# 120 - Rigger is language and process agnostic

**Goal:** rigger provides tools to agents (the context graph, the event store courier, worktrees,
the blast radius, the dash, hooks, MCP, the verdict and gate contracts) and executes the workflow
definition; it prescribes nothing about how work is done except by enforcing that definition. The
prompt-surface audit of 2026-10-20 (`~/.cache/rigger/drafts/op/prompt-surface-audit.md`)
inventories every string the binary puts in front of an agent; this spec takes the six places below,
and spec 121 takes the audit's remaining prescriptions and the boundary gate (OUT OF SCOPE names
them). The built-in instruction layer has no override: `compose`
(`crates/rigger-domain/src/instructions.rs:50-62`) injects every `BUILTIN` entry (:32-41) under
`HEADING` with the suffix "(non-negotiable)" (:54), `render` prints "Built-in (every agent rigger
spawns, not removable)" (:68), and the module doc says "no configuration removes it" (:11). And the
review ceremony is one fixed shape: `ReviewPanel` (`crates/rigger-domain/src/config.rs:355`) holds
one lens list, one `adversary` and one `adjudicator`, and `review_unit`
(`crates/rigger-conductor/src/conductor.rs:4773`) runs them as three fixed tiers (:4829, :4856,
:4885), so a consumer cannot add a second adversary or a lens after the adversary.
A third place, smaller, is the built-in bodies naming Rust vocabulary (MARKERS lists it).
The fourth is `REVIEWER_DISCIPLINE` (`conductor.rs:12529`), the prose every reviewer prompt
carries, which names a toolchain (`cargo test -p <crate> <test>`, `CARGO_TARGET_DIR`) and guides reviewers
with no override. The fifth is `RIGGER_COMMUNICATION` (`conductor.rs:12446-12464`), the courier
discipline `build_system_prompt` (:12476-12481) appends to every spawn with no override: a work
cadence (record at once, check peers between actions, supersede explicitly) that no gate or
definition enforces. The sixth is the critic's method: `spec_critique_prompt`
(`crates/rigger-domain/src/review.rs:277-335`) and the DAG critique's prompt bake the stance, Rules 7
and 8, the ownership defects, the two hunts and the ban into the binary, so a consumer's adversary
persona cannot change how a spec or plan is critiqued.
This spec makes the built-in layer overridable by name and the review panel an ordered sequence of
steps of the three official kinds, with the existing map form meaning the canonical sequence.

## Design

**THE RULE, decided here.** The handbook states it in two sentences, pinned by criterion 1's test
by their words (whitespace-normalized):

- S1: "Rigger is language and process agnostic: the binary provides tools to agents and executes
  the workflow definition, and it prescribes nothing about how work is done except by enforcing
  that definition."
- S2: "Prompts are guidance and logic is execution: the built-in instruction layer is best practice
  a consumer may replace or switch off, never a procedure standing in for a gate and never specific
  to a language."

Around them `docs/handbook/authoring-loops.md` gains a `## What rigger prescribes` section (after
the page's opening paragraph): the tools the binary provides, that a consumer may add any number of
stages, gates and review steps and rigger runs them, that gate template sets are the consumer's
choice at `rigger init`, and the official review vocabulary (lenses, adversary, adjudicator), and the tools' current
language coverage (the review cache's `CARGO_TARGET_DIR`, the grounder's Rust-only symbol
extraction). No
enumeration of cases or guarantees beyond these is owed. The ceremony section's five-step example is
`lenses [a, b]`, `adversary x`, `lenses [c]`, `adversary x`, `adjudicator z`, its one adjudicator
step last. The fenced `yaml` block is a `defaults:` fragment (`defaults:` holding `review:` holding
`steps:` with the five entries). Criterion 1's test splices it under a minimal workflow fixture
declaring agents a, b, c, x and z and one stage, runs `config_store::parse_workflow` then
`Config::validate`, and reads `workflow.defaults.review.steps()` as five. The test builds a
`Config { agents, workflow, .. }` with an `AgentDef` per id, z's body carrying the verdict line
`lint_gating_verdict_lines` (`crates/rigger-config-files/src/config.rs:73-76`) checks; the test runs
`parse_workflow` and `Config::validate`, not the lint.


**BASE, decided here.** This spec lands on `rigger-run` after PR #117 (the removal of mutation
testing from the core instruction layer and the personas) is merged to main and merged into
`rigger-run`. On that tree the bodies name no gate's procedure: the review
procedures they keep (the sentence walk, the one-home rule, the test-side shape search and each
lens's re-derivation) are guidance to authors and reviewers that no declared gate performs, which a
consumer replaces or switches off through criteria 2 and 3; S2's clause names a hand procedure that
re-creates a declared gate's function, which PR #117 removed; the probe rule and its tokens are gone at the launch base. The tree is named by the run's launch
record (the merge of PR #117 into `rigger-run`), the citations staying those read at 1185c8c6, and the launch
record confirms, before the run starts, that `REVIEWER_DISCIPLINE` at the launch base holds the five
sentences REVIEWER CONTRACT quotes verbatim (the fifth in its launch-base wording), is `pub`, and that the shipped-text helper exists.
Criterion 12 removes the language vocabulary that remains (the MARKERS list). Criterion 12 owns the
rewording and the edit of every existing test that pins a reworded phrase (`instructions.rs:380`
"a test fn, a helper", `:446` "the `//!` module doc", and any other
test a reworded phrase breaks); criteria 2 to 5 own no body text. The bodies drop every language construct
they name, each with its replacement: `#[allow]` (a lint suppression); `fn` (function); `crate` and
`crates` (package); `impl` (implementation); `trait` (interface); the `//!` module doc (the module
doc); `matches!`, a `match` with an arm per member and the match-guard condition (a pattern test, a
case statement with an arm per member, a pattern guard); each branch of a match (each branch of a
case statement); the match-arm guard (a case-arm guard); the argument-token examples
`create_dir_all(root.join(` and `for_extension("rs")` (a path built from a root and a name, a
file-extension argument); rustdoc's doc links (`[...]`, `working-discipline.md:179`)
(cross-references); the wasm `core` lane sentence (the inner layer compiles without the outer
layers); "on any feature lane" (`working-discipline.md:208`) becomes "under any build
configuration"; the project's package dependency chain (dropped, leaving: package dependencies point one
way, pinned by a structural test); the English "to match its output" (`engineering-principles.md:41`)
becomes "to fit its output"; in `REVIEWER_DISCIPLINE`, the third sentence is split into the two sentences
REVIEWER CONTRACT quotes and the fifth reworded as it quotes (the targeted test run and the build cache replacing `cargo test -p <crate> <test>` and
`CARGO_TARGET_DIR`); the constant is `pub` and the shared shipped-text helper exists at the launch base, both from
PR #117, and criterion 12 reads both texts through that helper; and rigger's own anchors, both criterion 12's: the helpers
sentence (`working-discipline.md:312-335`, "exist in this repository's `.claude/agents/`") becomes
"the `lookup` and `verify` helpers rigger scaffolds into `.claude/agents/`"; the adapter list at
`engineering-principles.md:9-11` (segmented stores, graph projector, headless host) becomes generic
examples (a store, a projector, a host); the domain list at `engineering-principles.md:7-8` (specs,
units, runs, decisions, the fold, the review verdict) becomes "the domain's entities, use cases and
rules"; the host name at `:9` becomes "the agent host"; `engineering-principles.md:30` (the simplification audit
sentence) is dropped; `:21` "the conformance suites" becomes "a port's conformance tests"; `:23`
"conductor code" becomes "use-case code"; `:15-16` "the dependency order below" goes with the
chain; `engineering-principles.md:24-25` "the pure `core` lane keeps compiling without them" (and
the inner layer keeps compiling without them); every sentence in either body that spells "the
SDET lens" or "the architecture lens" at the launch base (`working-discipline.md:88`, :177, :292,
:295 and :299-300 at 1185c8c6) is reworded so each becomes "the reviewing lens whose persona
assigns the duty", and later "the lens" references stay as written, so the routing rule stays in the text; criterion 12 edits no persona.
This repository's personas already assign them: `.rigger/agents/sdet.md:20-22` (the shape search
and the sentence walk re-derivation) and `.rigger/agents/architecture-reviewer.md:16` (the one-home
and member search), read at 1185c8c6; a missing assignment in any project is its operator's edit
between runs, never a unit's, so no definition drift arises. This enumeration is the closed anchor set criterion 12 rewords. Its unit also
walks both bodies for any other term naming this repository's own code as one consumer project
(its crates, modules, adapters, paths, personas, helpers, gates, lanes, host) and lists each in its
hand-over for the operator; that listing raises no finding. Rigger's product vocabulary every
consumer's agents use is kept (event types such as `DecisionMade` and `LessonLearned`, `rigger`
verbs and the graph's id syntax, the unit, run, attempt and review kinds). `<file>::<item>` stays: it is the graph's node id form
(`rigger graph --around <file>::<item>`), rigger's own syntax, so `::` is no marker. Any marker hit at the launch base
outside this list is reworded by the same token rule, its replacement the generic noun its line
already pairs with it, and listed in the hand-over.

**MARKERS, decided here.** Criterion 12's test lowercases each `BUILTIN` body and
`REVIEWER_DISCIPLINE`. A word marker (`fn`,
`crate`, `impl`, `trait`, `match`, `matches`, `cargo`, `clippy`, `rustfmt`, `wasm`, `rs`, `rust`, `rustdoc`,
`create_dir_all`, `for_extension`; the token `rs` covers `.rs` and `("rs")`) hits when it, or it followed by `s`,
equals a token of the body split on every character that is not a letter, a digit or `_`, so
`create_dir_all` is one token, so a
backticked word is a token and `implementation` is no hit. A syntax marker (`#[`, `//!`,
`cargo.toml`) hits as a substring. Three phrase markers, "the sdet lens",
"the architecture lens" and "feature lane", hit as substrings of the lowercased text. The list lives in the test. A word marker also hits when it equals any
`_`-separated part of a token, so `cargo_target_dir` hits `cargo`; `create_dir_all` and
`for_extension` stay whole-token markers. A word marker hit in ordinary English is
reworded too (BASE lists each), so the rule stays a token rule. `macro`, `closure`, `static`, `if`, `while`,
`&&`, `||` and the iterator verbs (`filter`, `any`, `all`, `retain`, `position`, `find`) stay: they
are constructs shared across languages, and the rule bans a language's own vocabulary, not programming vocabulary. SQL, a query
string, a conjunct and `IN (..)` stay too: a query language used from any host language is not the
host language's vocabulary.





**REVIEWER CONTRACT, decided here.** `REVIEWER_DISCIPLINE` has five sentences at the launch
base. Criterion 12, owning the words, cuts the third ("To reproduce a specific suspicion - one named
test, a reversion probe, a single-crate `cargo test -p <crate> <test>` - create your own throwaway
scratch worktree and run it there, building into the CARGO_TARGET_DIR your spawn carries: the
unit's review cache, never the cache the unit's gates use.") into two sentences: "To reproduce a
specific suspicion - one named test, a reversion probe, a single targeted test run through the
project's test command - create your own throwaway scratch worktree and run it there." and "A
reproduction builds into the build cache your spawn carries: the unit's review cache, never the
cache the unit's gates use." Criterion 12 also rewords the fifth sentence, at the launch base
"Never run a gate's own tool, directly or through a verify helper: the gate evidence in your prompt
is that gate's verdict, and a reviewer's run only repeats it at review cost.", to "Never run a gate's
command as the gate runs it, directly or through a verify helper: the gate evidence in your prompt is
that gate's verdict, and a reviewer's run only repeats it at review cost; the one targeted
reproduction above is not a gate run." A gate's own tool means its command as the gate runs it (its
whole battery or sweep); the one targeted reproduction through the project's test command is the
stated exception, so the two moved sentences agree. Any mismatch with the quoted text stops the launch for a Design amendment
before a unit spawns. Criterion 13 then moves three sentences verbatim into
`working-discipline.md` under a `## When you review` heading, the section sitting after `## Hand
over a tree the gates only confirm` and before `## Fan out the mechanical work`, in this order: "Never re-run the gate
battery, or any gate the gate evidence shows green on the tree under review: the gates are the
instrument that grades the work, and a review judges what they cannot see.", the reproduction
sentence criterion 12 wrote, and the fifth sentence criterion 12 reworded. The constant keeps, in this order, what the harness enforces or provides: "Never write
to this unit's own worktree - it is the tree being judged, not yours to edit.", the build-cache
sentence criterion 12 wrote, and "Leave the unit's worktree exactly as you found it." (both worktree sentences are what `guard_review_round_tree` enforces). The build-cache sentence
stands alone: it names the cache the harness provides and holds whether or not the body is in
force. Criterion 13's tests pin only where the sentences sit: the
three under `## When you review` in the working-discipline body and the constant's three kept
sentences. Their overridability is criterion 3's: its two-file switch-off fixture shows the `## When
you review` sentences gone once working-discipline is off, and criterion 2's replace fixture is the
other route.
`review_protocol` and `run_adjudicator` keep appending the constant; the moved sentences reach a
reviewer through `compose`. Criterion 12 edits, in place, every pin whose expected text it
rewords, each sentence staying where it sits in the constant (conductor.rs tests,
`tests/unified_traversal_grounding.rs:913-973` where they quote the third sentence); criterion 13
owns the split, the heading, every pin that moves because a sentence changes place from the constant
to the body, and the new pins of the kept trio; criterion 12 owns the words. The `verify` bullet's sentence under
`## Fan out the mechanical work`, "A reviewer never sends it the gate battery: the gate evidence in
its prompt already proves what the gates ran.", is cut by criterion 13 in the same commit as the
move, its fact then stated once under `## When you review`; no test pins it; criterion 12 leaves it
as written. `gate_evidence_block` (`conductor.rs:594-597`) repeats the second and third sentences in
every review prompt, toolchain words included; criterion 13 cuts that copy in the same commit as the
move, keeping the evidence rows and their framing sentence ("every gate below already ran on exactly
this commit"), and owns its pins; after the cut no toolchain word remains in it.

**BUILT-IN OVERRIDE BY NAME SHADOWING, decided here so no unit has to.** One pure function,
`instructions::resolve_layers(operator: &[Instruction])`, decides every built-in's state and the
remaining operator files; `compose`, `render` and `shadow_advisories` all read it, none re-derives
it. For each `BUILTIN` entry in order: an operator `Instruction` whose `name` equals the built-in's
name byte for byte (case-sensitive, never trimmed) with a body that is not empty after trimming a
leading U+FEFF and Unicode White_Space REPLACES it (state `Replaced`); one whose body is empty after
that trim SWITCHES IT OFF
(state `SwitchedOff`); otherwise it stays `BuiltIn`. A `Replaced` body is printed by `compose`
and `render` with its leading U+FEFF stripped. A shadowing file never also appears in the
operator layer. Every other operator file appends after the built-ins in filename order, as today.
A loaded near name (a `.md` file whose stem differs: `engineering-principles .md`, whose stem
carries a trailing space, or `Engineering-Principles.md`) shadows nothing and appends; `render`
lists it under the operator heading, which is how an operator sees it did not shadow; a file under
`.rigger/instructions/` without the `.md` extension is ignored as at HEAD and named in no report.

- `compose` prints `HEADING` bare (the " (non-negotiable)" suffix goes; criterion 2 owns the
  edit of every existing test that pins `HEADING` followed by " (non-negotiable)";
  `RIGGER_COMMUNICATION`'s heading and its pins are criterion 14's), then each built-in section
  under `## <name>`: the built-in body (`BuiltIn`), the operator body (`Replaced`), or nothing at all,
  heading included (`SwitchedOff`); then the unshadowed operator files. `HEADING` is printed even
  when every built-in is switched off and no operator file remains (`rigger prime` and the tests key
  on it). Criterion 2 lands the replace arm, criterion 3 the `SwitchedOff` arm.
- `render` prints `HEADING`, then `Built-in:`, then each built-in as `## <name> (built-in)`,
  `## <name> (replaced by .rigger/instructions/<name>.md)` followed by the operator body, or
  `## <name> (switched off by .rigger/instructions/<name>.md)` with no body; then the operator
  heading and the unshadowed operator files, `(none)` when none remain. The "not removable" label
  goes.
- `shadow_advisories(operator) -> Vec<String>` returns one line per non-`BuiltIn` built-in:
  `warning: built-in instruction <name> is replaced by .rigger/instructions/<name>.md` or `... is
  switched off by .rigger/instructions/<name>.md` (the `warning:` prefix `cmd_validate`'s other
  advisories carry). `cmd_validate` (`src/cli/validate.rs:140`) prints
  each to stderr beside its other advisories (:229) and never changes its exit status. `cmd_validate` passes it
  `cfg.instructions`, already loaded by `load_with_gate_requirements` (`validate.rs:160-163`); the
  directory is read no second time.
- `BUILTIN` SURFACE, decided here: criterion 5's README assertion and criterion 12's MARKERS assertion
  are the only two new assertions that iterate every `BUILTIN` entry, each excluding the other; the
  HEAD test `an_empty_persona_still_receives_every_built_in_layer` (`instructions.rs:131-147`) is
  pre-existing and exempt. Criteria 2 and 3 assert over named entries and read only `compose`'s
  output, and criterion 4 reads only `render`'s, `shadow_advisories`' and
  `instructions_in_force_line`'s output over a fixture of named entries ("each built-in" there means
  each entry of that fixture).
- Before criterion 3 lands, an empty same-named file leaves the built-in `BuiltIn` and appends as an
  operator file, as at HEAD.
- Until criterion 4 lands, `render` and `instructions_in_force_line` print as at HEAD, a shadowing
  file listed as an operator file.
- The module-doc sentence at `instructions.rs:10-12` ("no configuration removes it", :11) has one
  owner: criterion 2 rewrites it into the replace rule, keeping "reaches every agent rigger spawns";
  criterion 3 appends the switch-off clause to the sentence criterion 2 wrote and qualifies "reaches
  every agent" with "unless switched off".
- `SCAFFOLD_INSTRUCTIONS_README` (`src/cli/setup.rs:1499`) documents the mechanism and names every
  built-in by file name; this repository's own `.rigger/instructions/README.md`, today byte-equal
  to the scaffold text, is rewritten to the new text. Another project's README stays as written
  (`write_if_absent`, `src/cli/setup.rs:157-162`, never rewrites one); its `render` and `validate`
  report the override regardless.
- `instructions_in_force_line` (`src/cli/setup.rs:1410-1426`), the first line `rigger prime` prints,
  reads `resolve_layers`: it lists only the built-ins in force, in `BUILTIN` order, a replaced one as
  `<name> (replaced)`, a switched-off one omitted (`none` when no built-in is in force), and counts
  only the unshadowed operator files in its `(+N operator files)` suffix.
- Unchanged, decided: the tests reading `BUILTIN` that pin no reworded phrase
  (`src/cli/setup.rs:3484`, `crates/rigger-domain/src/docs.rs:2525`);
  `load_instructions` (`crates/rigger-config-files/src/config_store.rs:92-123`). Criterion 14 removes
  `RIGGER_COMMUNICATION`'s "(non-negotiable)" heading (`conductor.rs:12446`) with the constant, and after criterion 2 no prompt keeps the suffix.


**THE COMMUNICATION DISCIPLINE IS A BUILT-IN, decided here.** Criterion 14 moves
`RIGGER_COMMUNICATION`'s body (every line after its heading line, verbatim) into a third `BUILTIN`
entry named `communication`, after `working-discipline`; deletes the constant with its heading (the
"(non-negotiable)" wording goes: the discipline is guidance); and deletes the free
function `build_system_prompt`; `RunCtx::build_system_prompt` (`conductor.rs:10792`) stays as the
one method every spawn path reaches (five callers at the launch base: `re_plan`,
`reviewer_spawn_opts`, `run_single_stage`, `run_speculation` and
`spawn_conflict_resolution_implementer`) and now returns `instructions::compose(persona, operator)`
alone, its doc (:10784-10791) rewritten by criterion 14; the canary runner and the spec critic
(`src/cli/critique.rs:188`) call `instructions::compose` directly, `compose`'s
doc gaining one sentence naming it the one authority for every spawn's system prompt (criterion 3's
later edit of that doc keeps the sentence). Every spawn's prompt carries the discipline once, as the
`## communication` section, and `resolve_layers` governs it like the other two once criteria 2 and 3
land; criterion 2's replace fixture names `communication` as the one replaced built-in. Criterion
14's test pins membership only: the entry's name and body, `RunCtx::build_system_prompt`'s, the canary's and the
critic's prompt equal to `compose`'s output, and the removed heading "# Rigger communication
discipline (non-negotiable)" absent from it; `HEADING`'s own suffix stays until criterion 2, which
owns asserting that no "(non-negotiable)" remains anywhere in the composed prompt. The JSON shapes stay in the task
instructions (`EMIT_PROTOCOL`, `review_protocol`): they are the tool's contract. The moved body is
no added line under the Global constraints and has no MARKERS hit, so criterion 12's test stays
green. Criterion 14 owns every pin of the constant's heading or sentences and, in
`authoring-agents.md:85`, the words "It has two entries" (becoming "three") with a third numbered
item after :88 naming `communication` and its subject; criterion 3 keeps its sentences on those lines, landing after 14;
criterion 12's :87-88 wording lands before 14, and 14 adds its third item without rewording 12's
lines.

**THE CRITIQUE METHOD IS THE PERSONA'S, decided here.** Criterion 15 moves the method text verbatim
out of the binary into the `adversary` persona: `CRITIQUE_STANCE`'s skepticism and citation
sentences, `PLAN_CRITIQUE_RULES` (Rule 7, Rule 8, the blast-radius note), `CRITIQUE_OWNERSHIP`,
`CRITIQUE_HUNTS` and `CRITIQUE_BAN` become the body of the `## When you critique a spec or a plan`
section in `.rigger/agents/adversary.md` (after brief 93's two hunts, which stay) and in the
scaffold's adversary persona template in `src/cli/setup.rs`, the consumer's overridable default; the
persona is the workflow definition. The DAG critique's method residue moves with it: the unit-size
bullet (`conductor.rs:7876-7882`): its rule moves as persona prose without a number ("a unit over
the cap your prompt states is a BLOCKING finding"), and the DAG prompt keeps `unit_size_cap()`'s
text verbatim as its cap framing (no edit to the function or `MAX_UNIT_ADDED_LINES`, both spec
121's, which also moves the function's second sentence out of the binary), so the number has one
home, the scope sentence beginning "Reject
ONLY for" (:7927-7931) and the overlap parenthetical "judge ownership/coherence, not the overlap"
(:7913-7914); the opener's citation of `docs/handbook/authoring-loops.md` rules 7-8 becomes "the
decomposition rules in your persona", since a consumer repository has no such page.
`PLAN_PROTOCOL`'s copy of the cap and `MAX_UNIT_ADDED_LINES` are spec 121's. The binary keeps, in both prompts, the task framing (what is
under critique: the spec path and text verbatim, or the proposed DAG and the cap framing `unit_size_cap()` renders; the sentence "In
your persona's critique rules a unit reads as a criterion, and a reject as a BLOCKING finding"
replacing `CRITIQUE_STANCE`'s "In the rules below ..." sentence, with its trailing "The rules:"
header dropped), the voided-duties clause in `spec_critique_prompt` only (the DAG prompt never had
one and gains none) and `CRITIQUE_OUTPUT_CONTRACT` (the finding-line grammar and the verdict line the
binary parses: the tool's contract). The DAG prompt keeps its own verdict and cause paragraph
(`conductor.rs:7926-7941`, minus the moved "Reject ONLY for" sentence) as its output contract, and
its prior-reject block and unit and overlap list as its task framing; it gains neither
`CRITIQUE_OUTPUT_CONTRACT` nor a voided-duties clause, both of which stay in `spec_critique_prompt`
only. The moved constants are deleted; pins of their sentences move
to the template's section through the shipped-text helper; the critic's spawn path and
`NO_CRITIC_CLAUSE` stay criterion 10's. A persona without the section still critiques: the binary's
prompt stands alone, and the output contract decides the verdict. The DAG critique prompt reaches
two spawns, the adversary (`conductor.rs:8235`) and the gate's adjudicator (:8257), whose verdict
gates the fan-out. Criterion 15 therefore lands the method in both personas and both scaffold
templates, each in its role's voice under `## When you critique a spec or a plan`: the adversary's
section carries the stance, the hunts, Rules 7 and 8, the blast-radius note, the ownership defects
and the ban (how to find); the section names which parts apply to a spec only (the two hunts, the
ownership defects and the ban) and which to both a spec and a DAG (the stance, Rules 7 and 8, the
blast-radius note), so the plan gate's scope is unchanged; the adjudicator's section carries the judging rule (a finding stands
when it names a Rule 7 ownership defect, a Rule 8 open disposition, a bundled or twin criterion, a
unit over the cap the prompt states, or a defect in a criterion's own text that no decomposition can
remove; mechanical file overlap alone never does), its fix being the kept cause rule's: a re-plan for
`decomposition-conflict`, the run stopping for `spec-ambiguity`; the ban on criterion edits stays in
the adversary's section only, where it belongs to the spec critique. Two role-voiced sections are not a duplicate under the one-home
rule: the home of a method is the persona of the role that applies it. This
repository's `.rigger/agents/*.md` are a consumer's copies of the scaffold, outside the one-home
rule: the template is the shipped default and the repository's persona is its owner's
configuration, free to diverge (this repository's already do), so no test holds a repository
persona's section equal to its template's. If
`.rigger/agents/adjudicator.md` already carries a plan-critique passage, criterion 15 extends it into
that section rather than adding a second. The opener "the decomposition rules in your persona" is
then true for both spawns.


**INSTRUCTION STATE PLACEMENT, decided here.** The authoritative state is the files under
`.rigger/instructions/` on disk. `config_store::load_with_gate_requirements` reads them once per
driver process into `Config::instructions` (`config_store.rs:75`); `compose` resolves shadowing on
every call from that slice. No composed prompt and no shadow resolution is cached anywhere: a
memoized resolution kept across spawns is NOT an implementation of this design. The definition pin
already covers the files (`definition_hash` folds every loaded instruction, `src/cli/mod.rs:1006-1011`),
so an operator file added, emptied or deleted mid-run halts the next step as a definition drift;
under `--rebase-definition` the next process composes from the files as they then are. Criterion 15
rewrites `.rigger/agents/adversary.md` and `.rigger/agents/adjudicator.md`, both of which
`definition_hash` (`src/cli/mod.rs:975-1010`) folds
into the run's definition pin (criterion 5's `README.md` rewrite does not: `load_instructions`
excludes it). So criterion 15 lands in its own batch, after criterion 10 and before criterion 1, and
at its integration the operator relaunches the driver by adoption with `--rebase-definition`, the
procedure every mid-run definition change takes; the drift halt the pin raises at that boundary is
the expected signal, not a defect.

**INTERIM SENTENCES, decided here.** A handbook, README or module-doc sentence OUT OF SCOPE assigns
to a named criterion is rewritten when that criterion lands. An earlier unit whose behaviour makes
it false leaves it as it is: its hand-over lists the sentence as assigned to that criterion, and its
reviewers raise no finding on it. The assignments: `authoring-agents.md:85`, `:90` and
the switch-off clause appended to the module-doc sentence criterion 2 wrote to 3; `:92` and
`instructions.rs:10-12` to 2; `:94` to 4; `:87-88` to 12; the instructions README to 5;
`agentic-sdlc.md:62-68`, `README.md:19` and `:27` and the `authoring-loops.md` lines to 1; `best-practices.md:25` to 1;
`docs.rs:552-553` to 10; `docs.rs:2514-2518` ("every spawn receives") to 3; `compose`'s doc (`instructions.rs:47-49`) and `BUILTIN`'s doc (:24) to 3;
`render`'s doc (:64) to 4; the `ReviewPanel` struct doc (`crates/rigger-domain/src/config.rs:347-352`)
to 7; the `spawn.rs` docs of `spawn_id`, `ROLE_ADVERSARY`, `ROLE_ADJUDICATOR` and `lens_role` (one
fixed token per kind, the tier numbers) to 8; `REVIEWER_DISCIPLINE`'s doc
(`conductor.rs:12515-12528`) to 13; `run_adversary`'s and `run_adjudicator`'s roster docs
(`conductor.rs:7673-7676`, :7727-7730) to 6; the free `build_system_prompt`'s and
`RIGGER_COMMUNICATION`'s docs are criterion 14's, deleted with them, and
`RunCtx::build_system_prompt`'s doc is criterion 14's, rewritten; `review_protocol`'s doc
(`conductor.rs:12483-12505`) is criterion 8's; `review_roster`'s and `adjudicator_roster`'s docs
(`review.rs:250-265`) are criterion 6's; the docs of the deleted method constants (`review.rs:273-276`,
:293, :306, :311, :322) are criterion 15's, deleted with them; `authoring-agents.md:85`'s entry
count and the third item after `:88` are criterion 14's; `:85`'s "reaches every agent" wording stays
criterion 3's; the layer diagram at `authoring-agents.md:82` is criterion 14's, rewritten "persona +
built-in instructions (communication among them) + operator instructions"; the module-doc diagram
at `instructions.rs:5-8` is criterion 14's: it drops the parenthetical "(+ the communication
discipline, appended by the conductor)" and names the communication discipline as a built-in entry.




**THE REVIEW PANEL IS A SEQUENCE, decided here.** A panel is an ordered list of steps, each exactly
one official kind:

- `lenses: [a, b, ..]` - the agents run concurrently, each recording findings through
  `review_protocol` (`conductor.rs:12506`);
- `adversary: x` - one agent that reads every finding recorded so far in the round and records its
  own;
- `adjudicator: z` - one agent whose verdict line (`REQUIRED_PROTOCOL`, `conductor.rs:12535`) the
  conductor reads.

A panel in either form, and its `tiers.light`, has AT MOST ONE `Adjudicator` step, and it is the LAST
step. The consumer extends the ceremony with `Lenses` and `Adversary` steps, any number in any order
before the one adjudicator.

Type shape: `ReviewStep` is `enum { Lenses(Vec<String>), Adversary(String), Adjudicator(String) }`.
`ReviewPanel` keeps `lenses`, `adversary`, `adjudicator` and `tiers`, and gains
`steps: Option<Vec<ReviewStepDecl>>`, where `ReviewStepDecl` is a `deny_unknown_fields` struct of
three optional keys (`lenses`, `adversary`, `adjudicator`) that `Config::validate` requires to set
exactly one; no serde enum representation is relied on. `ReviewPanel::steps() -> Vec<ReviewStep>`
is the ONE authority every reader goes through: the declared list when `steps` is `Some`, else the
CANONICAL SEQUENCE of the map form - `Lenses(lenses)` when `lenses` is non-empty, then
`Adversary(adversary)` when set, then `Adjudicator(adjudicator)` when set. `is_empty`,
`agent_ids`, `full_roster` and `gating_agent_ids` (`config.rs:381`, :396, :407, :419) derive from
`steps()` (criterion 6's, today's shape over the canonical sequence). Criterion 7 makes the last
three return each agent id once, in first-step order, however many steps name it (a map form naming
one agent as lens and adjudicator included); under `tiers`, `agent_ids` and `gating_agent_ids` list
the full panel's ids first, then the light panel's ids not already listed, each once. So the workflow-definition grounder (`reviewers_of`,
`crates/rigger-grounder/src/grounder/workflowdef.rs:144`) reads the adjudicator step with no change
of its own, naming an agent once. `lint_gating_verdict_lines` and `unbounded_wall_clock_advisory` read the
config-level `gating_agent_ids(cfg)` (`crates/rigger-config-files/src/config.rs:21`), a `BTreeSet`,
so each already names an agent once and is unchanged. The advisory sums nothing: it names the adjudicator step's agent when it carries no per-agent bound under an unbounded default, as today. `reviewers_of` (`workflowdef.rs:144-157`) keeps its two lists, the full panel's
`REVIEWS` edges and the light panel's `REVIEWS_LIGHT` edges, never unioned: each agent gets at most one `REVIEWS` and one `REVIEWS_LIGHT`
edge, through `extract`'s link dedup (`:114`), so an agent in both panels keeps one edge of each kind; "full panel first" is
about `agent_ids` and `gating_agent_ids` only. Criterion 7 owns the dedup and `reviewers_of`'s naming-once behaviour through the accessors, with its tests; none of it is criterion
6's or 10's. `steps: []` is an empty panel, exactly as `review: {}` is: no review, and on a stage it
inherits `defaults.review` through `effective_review_panel` (`config.rs:958`) as an empty map does.

Criterion 7 adds the `Some` arm of `ReviewPanel::steps` (each `ReviewStepDecl` converted to its
`ReviewStep`) and its list-form test asserts `steps()` equals the declared list; criterion 6's
`steps()` has the canonical arm only.

**CANONICAL IDENTITY SURFACE, decided here.** Criterion 6's identity claim compares, for each
fixture, the ordered list of (spawn id, `SpawnOpts.reviews`, review task prompt, verdict) a
recording driver receives against the same list on the tree before criterion 6, captured as a
golden fixture in the test. The task prompt is `build_review_prompt`'s (`conductor.rs:11029`) and
`run_adjudicator`'s, NEVER the system prompt, which criterion 2 changes (the suffix) and which is
not this surface. The fixtures: this repository's `.rigger/workflow.yml` panel, a lenses-only panel,
a lenses-plus-adjudicator panel (the DROPPED case: the adversary is absent, so the adjudicator's
roster names no `adversary`), and a `tiers` panel routed both light and full. The existing roster
pins (`conductor.rs:32877-32885`) and the replay tests stay green unchanged. Criterion 6's loop
reads the adjudicator step's verdict exactly as at HEAD (:4911-4915, :5033-5034), so the identity
holds on a converged reject too. Steps are compared in step order; within one `Lenses` step the
spawns are compared sorted by spawn id, since `run_review_agents_concurrently` starts them on scoped
threads; only the order across steps is asserted.

**PARSE AND REFUSALS, decided here.** `Config::validate` (`config_store.rs:286`) refuses, naming
the scope (`defaults.review`, `defaults.review.tiers.light`, `stage "<name>" review`, `stage
"<name>" review.tiers.light`) and, for an entry, its 0-based index: a panel that sets `steps` and
any of `lenses`, `adversary`, `adjudicator` (`steps` is set when present, an empty list included;
the three map keys are set when non-empty; `steps: null` reads as absent (serde `None`)); an entry setting zero or two or more keys (an entry's key is set when non-empty,
so `{lenses: []}` is the zero-keys refusal; a present but empty second key is absent, so
`{lenses: [a], adversary: ""}` is a lens entry) (an unknown
key already fails the YAML parse); an agent named twice in one `lenses` step (in either form: two spawns of
one step would mint one spawn id, so no working workflow carries one); an agent id containing `@` anywhere the workflow declares one: the
`agents` map and every stage's `agents`, `agent`, `adversary` and `adjudicator` fields, every
`ReviewPanel` in either form and `tiers.light` (the step-token separator below; the standalone
fan-out stage mints `lens_role` from `st.agents` too, `conductor.rs:6954`), the message naming the
agent and the scope; a second `Adjudicator` step, or an
`Adjudicator` step followed by another step (permanent, criterion 7's). `validate_depth` (`config.rs:446`)
becomes: a panel with a `tiers` policy, and its `light` panel, each have at least one
`Adjudicator` step in `steps()`; each of its two messages gains the word "step" after "adjudicator", and the
`config_store.rs` tiers tests asserting the old text are criterion 7's to update. `tiers.light` takes either form. Until criterion 8 lands,
criterion 7 also refuses a list naming two steps of one kind (on that tree both would mint one
spawn id); criterion 8 lifts that refusal with the step tokens and turns criterion 7's
interim-refusal fixture into an acceptance assertion; that edit is 8's. Criterion 7's "any
sequence" therefore means any sequence of distinct kinds until criterion 8 lifts the repeated-kind
refusal. A list with an adversary before the
lenses is accepted from criterion 7 on: that adversary's roster is empty (ROSTERS). The same agent may appear in
several steps (two adversary steps naming one agent are two steps). A consumer workflow whose agent
id contains `@` is refused at load by design, the message naming the agent and telling the operator
to rename it; a run live on such an id halts at its next step with that refusal.

**STEP ROLE TOKENS, decided here.** A spawn id is `{unit}/{role}#{attempt}` and unique per role
(`crates/rigger-domain/src/spawn.rs:140`), so every step needs its own role token, and a map-form
panel must keep today's. `spawn::review_step_role(kind, agent, k)` mints, for the k-th step of its
kind in `steps()` (1-based, counted among steps of that kind only): the kind's existing token when
`k == 1` (`lens_role(agent)` for each lens, `ROLE_ADVERSARY`, `ROLE_ADJUDICATOR`; `spawn.rs:115`,
:95, :97), and that token followed by `@<k>` when `k >= 2` (`lens:c@2`, `adversary@2`). `@` is
neither `/`, `#` nor `~`, so `spawn_role` (`spawn.rs:222`) and `unit_of` stay correct. The ONE
parser `spawn::review_step_of(role) -> Option<(ReviewKind, u32)>` reads a token back (a bare token
is step 1; the suffix is `@` followed by an ASCII decimal number with no leading
zero and no sign, at least 2; any other suffix is no review token; a suffix
above `u32::MAX` is no review token in both readers; `@k` exists for the lens
and adversary kinds only, since one adjudicator step means `adjudicator@k` is never minted;
`review_step_of` returns `None` for any `adjudicator@k`); criterion 8 migrates
`SpawnResult::is_adjudicator` (`spawn.rs:653`) to it, and the other production classifiers are
criterion 9's and 10's by name (READERS lists every site). The token is the step's label
everywhere: the spawn id, the `role` argument of `run_reviewer` (`conductor.rs:7321`), whose `tier`
argument keeps the kind label (`lens`, `adversary`, `adjudicator`) so error and halt text reads as
at HEAD, `@k` reaching `degenerate_reviewer` (:7525) through `role`, which it already takes, the finding's `by` field (`review_protocol(actor)`), the session a later round resumes
(`continue_prior_session`, `conductor.rs:4569`, keyed by role). The tokens are a pure function of
`steps()` and the step's index: an in-memory counter of steps run so far is NOT an implementation
of the ordinal, since a resumed process starts it at zero. The plan-critique gate's and the
critique run's tokens (`review::critique_spawn_id`, `crates/rigger-domain/src/review.rs:392`) are
unchanged. `review_step_of` takes `spawn_role`'s output, the `~retry{n}` suffix already trimmed
(`spawn_role`, `spawn.rs:222-224`); a raw role is never passed to it.

**EXECUTION, decided here.** `review_unit` routes the panel exactly as today (`select_review_panel`,
`conductor.rs:4396`; `log_review_tier`, :4419), approves an empty panel at once (:4805-4807), then
runs `steps()` in order, one step at a time: a `Lenses` step through
`run_review_agents_concurrently` (:7172), an `Adversary` step through `run_adversary` (:7681), the
`Adjudicator` step through `run_adjudicator` (:7739), each under its step role token, every step
getting the round's block (`round_delta`, :4818-4825) as today. Criterion 6's loop computes
`step_roster(steps, index, mint)` per step and passes that `Vec<String>` to
`run_review_agents_concurrently`, `run_adversary` and `run_adjudicator` as each spawn's
`SpawnOpts.reviews` roster, never `panel.lenses` (:4826, :4872, :4895) and never `review_roster` or
`adjudicator_roster`, which stay only as wrappers for the standalone review stage and the
plan-critique gate. The adjudicator step, being last, is read exactly as at HEAD:
`split_reject` with the round object, the degenerate-verdict check, `continue_prior_session`,
`verdict_compensates` before the split (:4910), the `reviewed` stamp, and the speculation stamp
through `steps()` having an adjudicator. The tree guard runs as today: `guard_review_round_tree`
after the last step that ran, and `guard_review_round_tree_on_tier_err` when any step's spawn
errors.

**ROSTERS, decided here.** "A step carries the ids of every earlier step's agents" means its
`SpawnOpts.reviews` roster (`crates/rigger-domain/src/agent.rs:142`), the channel the thin driver
renders today; no prompt text changes. `review::step_roster(steps, index, mint)` takes the
token-minting function as its single seam and returns the tokens `mint` gives every spawn of every
step before `index`, in step order and, within a lens step, in declared
order. A `Lenses` step at any index has an empty roster, as `run_lens`'s is today
(`conductor.rs:7244`), since no reader renders a lens's roster (`workerLabel` ignores it,
`ROSTER_VERB` names no lens): `step_roster` returns the empty roster for a `Lenses` step; every
adversary and adjudicator step carries `step_roster`'s tokens. `review_roster` and `adjudicator_roster`
(`review.rs:257`, :265) become wrappers over `step_roster` on the canonical sequence, so the
standalone review stage and the plan-critique gate (`conductor.rs:6991-7008`, :8262) keep their
rosters through the one rule; `review_unit` never calls them. Criterion 6 is the one owner of `step_roster`, the roster's shape,
the wrappers and the loop's roster input, and lands it with a bare-token minter (today's tokens), so
on the canonical sequence it equals today's `review_roster` and `adjudicator_roster` and criterion
6's identity claim holds; it owns no token spelling. Criterion 8 supplies `review_step_role` as the
minter and owns only the tokens it mints, never the roster's shape. A later step reads earlier
steps' findings through the graph, as at HEAD: every lens and adversary records each finding live
as a `ReviewFinding` (`review_protocol`, `conductor.rs:12506`) and every later step's grounded
prompt (`graph_context`) retrieves them; the roster is the thin driver's label
(`agent.rs:132-142`), never the channel. `review_protocol`'s sentence that the adversary, the
adjudicator and the fellow reviewers see each finding (`conductor.rs:12511`) stays as written: it is
true of the canonical sequence, it is guidance, and the map form's prompts stay byte-identical; a
list-form ceremony reads it as its own later steps.

**THE ROUND'S VERDICT, decided here.** The one adjudicator step's verdict is the round's, read as
at HEAD (`split_reject`, `conductor.rs:4996`; a converged reject approves, :5033-5034; the
`reviewed#{attempt}` stamp, :4958, and the speculation winner's `reviewed#{lane}` stamp, :6758-6771,
carrying the effective panel's adjudicator alias). A sequence with no adjudicator step approves when
its last step ends, with no `reviewed` stamp, as an adjudicator-less map panel does today
(:4879-4884). A second judge is out of scope: the round's verdict is the one adjudicator's.

**READERS, decided here.**

Every production site at 1185c8c6 that mints, compares or classifies `ROLE_ADVERSARY`,
`ROLE_ADJUDICATOR`, `lens_role`, the `lens:` prefix, `is_adjudicator` or `spawn_role`, and its owner:

| Site | What it does | Owner |
| --- | --- | --- |
| `spawn.rs:654` `is_adjudicator` (read by `adjudication`, :663, the review-quality and finding-expiry folds, and the console, `console/mod.rs:262`) | classifies | 8 |
| `conductor.rs:7258`, :7265, :7696, :7703, :7774 (`run_lens`, `run_adversary`, `run_adjudicator`) | mints | 8, through `review_step_role` |
| `review.rs:258`, :268 (`review_roster`, `adjudicator_roster`) | mints the roster | 6 |
| `metrics.rs:240-245` `review_tier`, :257 `actor_tier`, :1060 and :1065 the adversary-only fold | classifies | 9 |
| `dash.rs:2537-2539` `stage_of_role` (called at :2357), :2554-2559 `role_and_agent` | classifies | 9 |
| `workflows/rigger.js:325-343` `roleOf`/`phaseOf`, :365 `PERSONA_VERB`, :384 `ROSTER_VERB` | classifies | 9 |
| `canary_store.rs:175`, :443, :460, :479, :490, :549, :551 | mints and keys tiers | 10 |
| `conductor.rs:13777` `recorded_session` | compares a whole token for equality | unchanged: one session per step token is the design |
| `metrics.rs:1024` spawn timing, :1289 tier seeds | keys by whole token; seeds kind names | unchanged: `adversary@2` is its own timing row |
| `conductor.rs:8228`, :8235, :8250 (plan-critique gate), `review.rs:393` `critique_spawn_id` | mints | unchanged: out of scope |
| `src/cli/eval.rs:94` | parses `--model` tier names | unchanged |

Between criterion 8 and criterion 9 an `@k` token falls to each criterion-9 reader's non-review arm
(metrics, dash and `rigger.js` misclassify it); accepted, and criterion 8's reviewers raise no
finding on it.



- `metrics::review_tier` and `actor_tier` (`crates/rigger-domain/src/metrics.rs:239`, :256) key cost
  and precision on the kind (`lens`, `adversary`, `adjudicator`); the adversary-only fold (:1060)
  counts a finding by any adversary-kind actor about files no lens-kind actor flagged, lens steps
  after the adversary included, since the metric reads the whole round;
  `finding_survival` stays keyed by the full token, so `adversary@2` is its own row.
- `dash::stage_of_role` and `role_and_agent` (`crates/rigger-dash/src/dash.rs:2534`, :2553) put every
  review-kind token in `Review` and group it under its kind, the agent label gaining ` step <k>`
  for `k >= 2`.
- `workflows/rigger.js`: `phaseOf` (:340), `PERSONA_VERB` (:365) and `ROSTER_VERB` (:384) look up
  the token with a trailing `@` followed by a decimal of at least 2 and at most 4294967295 (`u32::MAX`) with no
  leading zero, a larger suffix being no review token, never on
  an `adjudicator` token, stripped, the grammar `review_step_of` reads; the persona label
  keeps the full token. A lens step's roster stays empty; `workerLabel` (:463) renders a lens's base phrase as at
  HEAD.
- The console reads verdict marks through `SpawnResult::is_adjudicator`
  (`crates/rigger-console/src/console/mod.rs:262`), so it marks the adjudicator step's verdict with
  no change of its own.
- `wave::critic` (`crates/rigger-domain/src/wave.rs:204`) takes `defaults.review`'s first
  `Adversary` step; `NO_CRITIC_CLAUSE` (`wave.rs:198`) and the `rigger critique` help
  (`src/main.rs:341`) say "an adversary step in `defaults.review`" in place of
  `defaults.review.adversary`, so the advice holds in both forms; the rendered skill text
  (`docs.rs:552-553`) is reworded as OUT OF SCOPE decides.
- The canary: `canary_store::apply_model_pins` (`crates/rigger-conductor/src/canary_store.rs:168`)
  pins every agent of every step of the pinned kind; `run_canary` (:245) refuses a panel with no
  adjudicator step, runs the steps in order under their step tokens, each canary spawn carrying
  `step_roster(steps, index, review_step_role)`, the adjudicator's reversed run reversing the whole
  ordered finding list across steps as at HEAD, scores the lens tier over
  every lens step and the adversary tier over every adversary step, and judges the one adjudicator
  step as today (`adjudicate`, :541). `--model <tier>=<id>` keeps its three tier names (`src/cli/eval.rs:94`).
- Every direct reader of the panel's `lenses`, `adversary` and `adjudicator` FIELDS at 1185c8c6, and
  its owner, who makes it read `steps()`: `review_unit` (`conductor.rs:4826-4828`) and the
  speculation stamp (`!panel.adjudicator.is_empty()` and its alias, :6759, :6771) - criterion 6;
  the accessors `is_empty`, `full_roster`, `gating_agent_ids` (`config.rs:382`, :407-409, :421) -
  criterion 6, their dedup criterion 7's; `validate_depth` (:452, :462) - criterion 7; `wave::critic`
  (`wave.rs:208`) - criterion 10; the canary (`canary_store.rs:172-176`, :254, :442, :458-460, :549)
  - criterion 10. `rigger canary`'s empty check (`src/cli/eval.rs:204`) reads `is_empty`, so it is
  criterion 6's. Between criterion 7 and criterion 10 a list form is accepted by validation and
  carries this degradation: `wave::critic` finds no adversary in a list-form `defaults.review`, so
  `rigger critique` refuses as for a workflow naming no critic, and `run_canary` refuses a list-form
  panel as naming no adjudicator. With one adjudicator last, the speculation stamp's alias stays the
  effective panel's adjudicator, as at HEAD.
- The replay driver (`crates/rigger-driver/src/driver/replay.rs`) re-runs `review_unit` and answers
  each spawn from the log by id; it reads no role of its own, so the deterministic step tokens are
  what make it correct.
- No production code of the MCP server reads a review role (`crates/rigger-dash/src/mcpserver.rs`;
  its hits at :2264 and :2298 sit in tests, past `mod tests` at :1056): nothing changes there.

**OUT OF SCOPE, decided here.** A stage's own `adversary` and `adjudicator` fields (`config.rs:634`)
and everything reading them - the standalone fan-out review stage (`conductor.rs:6980-7070`), the
plan-critique gate (:8219-8262), `critique_gate_name` and `has_llm_verifier` (`wave.rs:186`, :305),
`plan_critique_gate_name` (`crates/rigger-config-files/src/config.rs:50`) - take no sequence and are unchanged;
criterion 6 adapts its calls into `run_review_agents_concurrently`, `run_adversary` and
`run_adjudicator` (`:6981-7010`) to pass the `review_roster` and `adjudicator_roster` wrappers, and
criterion 8 passes k = 1 there; its behaviour is unchanged.
The scaffolded workflow (`SCAFFOLD_WORKFLOW`, `src/cli/setup.rs`) and this repository's
`.rigger/workflow.yml:83-86` keep the map form. The workflow comment at `.rigger/workflow.yml:74-82`
stays as written: it describes this repository's own three-step panel, and editing the file would
change the definition hash. `review_unit`'s tier comments (`conductor.rs:4829`, :4856, :4885) are
criterion 6's to reword as steps. The `SCAFFOLD_WORKFLOW` doc and template comments
(`src/cli/setup.rs:1514-1524`) stay as written: they describe that template's own three-step panel. The per-unit build-cache naming
(`RunCtx::spawn_env` exports a `CARGO_TARGET_DIR`) is a toolchain coupling in code, out of this
spec's scope like the grounder's Rust-only symbol extraction; the handbook's `## What rigger
prescribes` section names both as the tools' current language coverage, which the rule binds and
the operator carries; the core-names-no-
toolchain constraint binds the lines this spec adds. Every other page under `docs/handbook/` (`using-rigger.md`,
`planning-field-guide.md`, `tools-and-context.md`, `best-practices.md` outside `:25`, the rest) and
every shipped skill carry no fixed-tier or unremovable-layer
wording and are untouched, except the critique skill's sentence at `docs.rs:552-553` ("the
plan-critique gate's adversary, else `defaults.review.adversary`"), which criterion 10 rewords to
"else an adversary step in `defaults.review`" and re-renders with the tree's binary. The pages this spec edits are hand-written: `authoring-loops.md` (its
opening, the `review:` comment at :54, the lifecycle line at :100, the new sections),
`agentic-sdlc.md:62-68` (the review section, retitled as a sequence whose default is the three
steps) and `docs/handbook/README.md:27` (the glossary's review-panel line) and `:19` (the "three-tier
adversarial review" phrase), `authoring-loops.md:106` and `docs/handbook/best-practices.md:25`
(the "three-tier shape" sentence, reworded to call the three steps the default sequence a consumer
may extend), all criterion 1's; and
`authoring-agents.md:85-96`, split by owner: `:94` (the report's states) is criterion 4's, `:87-88` (the bodies) criterion 12's; `:85` (reaches every agent), `:90` and the switch-off clause appended to
the module-doc sentence criterion 2 wrote are criterion 3's; `:92` (appended after the built-ins)
and the `instructions.rs:10-12` sentence (reaches every agent anywhere, no configuration removes it)
are criterion 2's. That split (2: `:92`; 3: `:85`'s "reaches every agent"
wording, `:90`; 4: `:94`; 12: `:87-88`; 14: the entry count on `:85`, the third item after `:88`,
and the layer diagram at `:82`) is the one authority over those named phrases of
`authoring-agents.md:82-96`, not over whole lines. `authoring-loops.md`'s
existing workflow example keeps the map form, so `tests/handbook_grounder_accuracy.rs`'s
`grounder:` check (:124-152) reads it unchanged; the five-step example sits in a fenced `yaml`
block under a `### The review ceremony` heading, the last subsection of `## What rigger
prescribes`, and carries no `grounder:` line. A
`--rebase-definition` that changes a unit's review panel mid-unit behaves as at HEAD, steps resuming
by role token, so a token whose agent or position changed under the rebase may resume another step's
recorded spawn or session, exactly as the bare adjudicator token does today when `adjudicator: z`
becomes `adjudicator: w`; the operator's remedy is to let the unit's open round finish, or reset the
unit, before rebasing a panel; this spec adds no rebase rule. Spec 121 owns the audit's remaining prescriptions: the graph-first lookup method and
the grep-guard hook (`write_lookup_pointer`, `hooks.rs:25-27`, the `lookup` helper); the helpers'
two homes (`HELPER_AGENTS` passed inline by `spawn_config_args`); the cadence clauses in
`EMIT_PROTOCOL`, `review_protocol`, `DECISIONS_HEADER` and the shim; the progress cadence in the four
host homes; `DOC_ONLY_ROUND`; the unit size cap in `PLAN_PROTOCOL` and `MAX_UNIT_ADDED_LINES`;
`PLAN_PROTOCOL`'s guidance sentences; the REQUIRED PATTERN audit sentences; `ROLE_SDET_AUTHOR`'s
undeclared spawn; the toolchain words in `workflows/rigger.js` and the `rigger_scratch` description;
the skills' house constraints and skill shadowing; the scaffold's language-bound implementer; and
the boundary gate, a registry of every prompt string the binary ships classed contract or framing.

**CONSTRAINTS WALK, decided here.**

| Mechanism | Corner | Decision |
| --- | --- | --- |
| shadowing | empty | no operator files: every built-in `BuiltIn`, output equal to HEAD's apart from the suffix and label |
| shadowing | repeated | two files cannot share a stem in one directory; `x.md` and `x .md` are two names, the second appends |
| shadowing | near name | trailing space or case change appends (no trimming) |
| shadowing | reverted | a file emptied then refilled: each load reads the body as it is; definition drift between |
| shadowing | DROPPED | a shadowing file deleted between spawns: drift halts the next step; rebased, the built-in returns from the next fresh spawn, a session resumed through `continue_prior_session` keeping its original system prompt, as for any operator file today |
| shadowing | concurrent | two units compose from the one `Config` slice; resolution is pure |
| shadowing | crash-resume, cold start | nothing cached; the next process loads the files and resolves again |
| shadowing | existing data | an older README stays; render and validate report the override |
| shadowing | existing same-named file on upgrade | a project's operator file whose stem equals a built-in name, appended at HEAD, becomes a replacement under the new binary with no drift halt (`definition_hash` folds the unchanged files), surfaced by the `validate` advisory; accepted |
| sequence | empty | `steps: []` is an empty panel (approve at once; a stage inherits `defaults.review`) (criterion 7's test) |
| sequence | one-agent lens step | a concurrent run of one; token `lens:a` or `lens:a@k` |
| sequence | repeated | one agent in two adversary steps: tokens `adversary`, `adversary@2`, two sessions; one agent twice in one lens step: refused |
| sequence | concurrent | lens agents within a step only; never two steps at once |
| sequence | crash-resume | tokens recomputed from `steps()`; every recorded step answers from the log |
| sequence | rebased map to list | the identical canonical spelling mints the same ids and replays |
| sequence | a changed sequence on rebase | as at HEAD, by token (out of scope) |
| sequence | risk routing moves a unit between the full and light panels across attempts | as at HEAD: `continue_prior_session` resumes by whole token, so a token naming a different agent in the other panel resumes that token's session (HEAD's bare `adversary` already does); accepted |
| sequence | existing store | old ids carry bare tokens, which parse as step 1 of their kind; metrics fold them as today; an old id whose agent name held `@<n>` reads as a step suffix, accepted as the mislabel of a store older than the load-time refusal, as for the criterion 8 to 9 window |
| sequence | `tiers.light` in list form | routed exactly as today; must have one adjudicator step, last |
| sequence | a second adjudicator, or one not last | refused at load, naming the scope (criterion 7) |

## Notes (non-criteria)

UNIT ORDER (the needs chain): 12, 13, 14, then 2, 3, 4, 5, then 6, 7, 8, 9, 10, then 15, then 1,
then 11.
Criteria 2-5 (instructions) and 6-10 (review) are independent chains; 1 needs 12, 13, 14, 15, 3, 7, 8 and 10; 13 needs 12; 14 needs 12; 5 needs 14; 10 needs 7 and 8; 11
needs all.

LANDING-ORDER SIMULATION (every ordered pair sharing a surface; "no" lines were fixed in the text):

- 12 before everything (surface the built-in bodies): text and one test, no code -> yes; 1 after 12
  so S2 is true on the tree.
- 13 after 12 (surface the reviewer sentences): 12 rewords them in the constant, 13 moves them; 1
  after 13 so S1 is true on the tree.
- 14 before 2-5 (surface `BUILTIN` and `compose`'s callers): 14 adds an entry and removes the
  append; 2-5 iterate `BUILTIN` -> yes
- 15 in its own batch before 1 (surface the two critique prompts and the adversary persona): no
  other criterion touches them; the definition pin moves at its boundary, rebased by the operator
  -> yes





- 2 then 3 (surface `compose`): 2 without 3 asserts only the replace arm and a near name -> yes. 3
  without 2 would need `resolve_layers` -> 3 lands after 2 by order; 3's text names only the off arm.
- 4 without 3 (surface `render`): 4 asserts the switched-off label, which needs 3 -> fixed by order
  (4 after 3); 4 owns no `compose` assertion.
- 5 without 3: the README describes the off arm -> fixed by order; 5's test reads text only.
- 6 without 7 (surface `steps()`, `review_unit`): 6 asserts the canonical sequence only -> yes.
- 6 after 2 (surface the spawns' prompts): 2 removes the `HEADING` suffix from every system prompt
  before 6 lands, so an identity over system prompts would fail 6's own text -> fixed by the
  CANONICAL IDENTITY SURFACE block: 6 compares task prompts against the tree before it, never HEAD.
- 7 without 8 (surface `config::load`): 7 asserts parse, refusal and the accessors' dedup only,
  never a run, and refuses a list naming two steps of one kind (one spawn id on that tree) -> yes;
  a list with an adversary before the lenses runs on that tree with an empty lens roster, as
  EXECUTION decides. 8 lifts the repeated-kind refusal.
- 8 after 7 (surface the spawn ids): 8 lifts the repeated-kind refusal and its fixtures run under
  its own tokens over criterion 6's loop and rosters, the adjudicator read as at HEAD -> yes.
- 10 needs 7 and 8 (its canary runs the steps under criterion 8's tokens): 10 after 8; before 8 the
  list form loads and `wave::critic` and the canary carry the READERS degradation.
- 9 without 8 (surface the token grammar): 9's fixtures carry `@k` tokens 8 mints and parses ->
  9 after 8; 8 owns `is_adjudicator`, so the console needs no clause in 9.
- 10 without 7: 10 parses list-form fixtures -> 10 after 7.
- 1 without 7: the five-step example must parse -> 1 after 7; 1 without 2/3 would state S2 about an
  override that does not exist -> 1 after 3.
- 6 and 7 both name `config.rs`'s `ReviewPanel`; 6 adds `ReviewStep` and `ReviewPanel::steps()`; 7 adds the
  `steps` field. A shared
  blast radius serializes them; ownership is split by the OWNS sentences.

PER-CRITERION CORNERS (beyond the CONSTRAINTS WALK table, which covers 2, 3, 6, 7 and 8; rosters are 6's):

- 1: the example block DROPPED from the page fails the parse clause; a rewrap never moves S1 or S2
  (read by words). Concurrent, crash-resume, cold start: out of scope - static text.
- 4: an empty operator layer renders `(none)`; an all-shadowing layer renders `(none)` too; no
  shadow prints no advisory; repeated `validate` prints the same lines.
- 5: a new `BUILTIN` entry fails the test until the README names it; an older project README is
  existing data, left as written.
- 6 (rosters): a sequence of one step: an empty roster; a lens agent in two lens steps appears twice, once per
  token; a DROPPED adversary (lenses plus adjudicator) leaves `adversary` out of the roster, as today.
- 9: a token with `@1` or `@x` is no review token (falls to the reader's non-review arm); a store
  mixing bare and `@k` tokens folds each by its kind.
- 10: a panel with no adversary step has no critic (as today); one agent named in two kinds under two `--model`
  pins: as at HEAD, pins apply in `BTreeMap` key order and the last applied wins; accepted.
- 11: out of scope for every corner - it runs the lanes once on the finished tree.
- 12: a new BUILTIN entry carrying a marker fails the test; static text, no other corner.
- 13: a sentence in both the constant and the body is a duplicate (one home); the adjudicator's
  prompt reaches the body through `compose` as every spawn does.

THREE RISKIEST CONTRADICTIONS, and how the text resolves them:

1. Byte-identical map form versus distinct ids for repeated kinds. A step token keyed on the global
   step ordinal would rename every spawn of every existing panel and orphan every recorded result.
   Resolved by STEP ROLE TOKENS: the ordinal counts within a kind and the first step of each kind
   keeps today's bare token, so the canonical sequence mints today's ids and an existing store's
   tokens parse as step 1.
2. New refusals versus "every existing workflow unchanged". Refusing a lens named twice in one step
   or an `@` in an id applies to the map form too, which could reject a workflow HEAD loads.
   Resolved by criterion 7 owning a load-unchanged check over this repository's
   `.rigger/workflow.yml` and every scaffold gate template set's rendered workflow, none of which
   carries either, and by recording why no working workflow can carry a duplicate (its two spawns
   share one id).
3. "Each step's prompt carries earlier agents" versus byte-identical prompts. Adding the roster to
   prompt text would change every map-form prompt. Resolved by ROSTERS: the carrier is the existing
   `SpawnOpts.reviews` roster, whose canonical value equals today's `review_roster` and
   `adjudicator_roster`; no prompt text changes.

TEST PLACEMENT (criterion -> test homes; the fixtures each test uses):

| Criterion | Test homes and fixtures |
| --- | --- |
| 1 | `tests/handbook_grounder_accuracy.rs` over the committed pages: S1 and S2 by words; the fenced `yaml` block under `### The review ceremony` parsed; `docs/handbook/agentic-sdlc.md`'s review section naming the sequence and the three kinds |
| 2 | `crates/rigger-domain/src/instructions.rs` tests through `resolve_layers`: one named built-in replaced; a trailing-space and a changed-case name appended |
| 3 | `instructions.rs` tests: one named built-in off, with a second named built-in off as a fixed two-file fixture (never iterating `BUILTIN`), leaving `HEADING` and the operator layer alone |
| 4 | `instructions.rs` tests for `render` and `shadow_advisories`; `src/cli/setup.rs` tests for `instructions_in_force_line`; a periphery test running the built binary's `instructions`, `validate` and `prime` over a temp project with one replacing and one empty file |
| 5 | `src/cli/setup.rs` tests iterating `BUILTIN` |
| 6 | `crates/rigger-domain/src/config.rs` tests for `steps` and the accessors' canonical shape; `review.rs` tests for `step_roster`, the roster shape over any minter (equal to `review_roster` and `adjudicator_roster` on each canonical sequence with the bare-token minter, a lens step at any index empty, and over the five-step sequence with a test minter the second adversary's roster naming both lens steps and the first adversary, the adjudicator's adding the second); `conductor.rs` tests over a recording driver and the golden fixtures |
| 7 | `crates/rigger-config-files/src/config_store.rs` tests (every refusal, the one-adjudicator-last refusal and the interim repeated-kind one included; the repo workflow and every scaffold template set's rendering); `crates/rigger-domain/src/config.rs` tests of the dedup and `workflowdef.rs` and `crates/rigger-config-files/src/config.rs` tests of `reviewers_of`, each over a map form naming one agent as lens and adjudicator, a list form naming one agent in two steps and a `tiers` panel (named once within each of `reviewers_of`'s two lists; once, full panel first, in the accessors); a periphery test loading a list-form `.rigger/workflow.yml`; a `config.rs` test where a stage with `review: { steps: [] }` resolves to `defaults.review` |
| 8 | `spawn.rs` tests (bare tokens parse as step 1, a `~retry2` respawn id, read through `spawn_role`, parses as its step, `@k` for lens and adversary kinds); `conductor.rs` tests over `lenses [a, b]`, `adversary x`, `lenses [c]`, `adversary x`, `adjudicator z` minting `lens:a`, `lens:b`, `adversary`, `lens:c@2`, `adversary@2`, `adjudicator`, the minted tokens appearing in its rosters and the adjudicator read as at HEAD; `replay.rs` tests resuming that sequence mid-round from the log |
| 9 | `metrics.rs` and `dash.rs` tests (a bare-token store folding to the prior `ReviewQuality`); `tests/phase_of_role_mapping_periphery.rs`; `tests/review_tier_roster_periphery.rs`; `tests/phase_of_role_mapping_periphery.rs` holds the one token table (bare, `@2`, `@1`, `@01`, `@4294967296`, `@x`, `adjudicator@2`) and asserts `rigger_domain::spawn::review_step_of` and the JS `roleOf`/`phaseOf` agree on every row |
| 10 | `wave.rs` and `canary_store.rs` tests; a periphery test running the built binary's `critique --help` for the adversary-step wording |
| 11 | the lanes script on the integrated tree |
| 12 | `src/cli/setup.rs` tests iterating `BUILTIN` and `REVIEWER_DISCIPLINE` under the MARKERS rule (the root crate reaches both; the shipped-text helper PR #117 adds is the one home of the texts); the existing tests BASE names, edited |
| 13 | `conductor.rs` tests pinning the constant's three sentences; `src/cli/setup.rs` tests pinning the `## When you review` heading and its sentences in the working-discipline body; the pins that move, edited |
| 14 | `instructions.rs` tests (the third entry's name and body); `conductor.rs` tests (the spawn path's prompt equals `compose`'s output, the `## communication` section once, no "(non-negotiable)"); `canary_store.rs`'s prompt test; the existing pins of the constant, edited |
| 15 | `review.rs` tests for `spec_critique_prompt`'s sections; `conductor.rs` tests for the DAG critique prompt; `src/cli/setup.rs` tests through the shipped-text helper for the template's section; the existing pins of the moved sentences, edited |

ARTIFACT OWNERSHIP (owner = the criterion of the first production caller):

| Artifact | Owner |
| --- | --- |
| `resolve_layers` and its `Replaced` state, `HEADING` printed bare, the module-doc replace sentence | 2 |
| the `SwitchedOff` state, the `SwitchedOff` arm inside `compose`, the module-doc clause that an empty same-named file switches a built-in off | 3 |
| `render`'s labels, `shadow_advisories`, its `cmd_validate` call, `instructions_in_force_line` reading `resolve_layers` | 4 |
| `SCAFFOLD_INSTRUCTIONS_README`, `.rigger/instructions/README.md` | 5 |
| `ReviewStep`, `ReviewPanel::steps`, the derived `is_empty`/`agent_ids`/`full_roster`/`gating_agent_ids` in today's shape over the canonical sequence, the one step loop in `review_unit` with the adjudicator read as at HEAD, the speculation stamp through `steps()`, `step_roster`, the `review_roster`/`adjudicator_roster` wrappers and the loop's roster input (`step_roster` per step) | 6 |
| `ReviewStepDecl`, `ReviewPanel.steps`, the `Some` arm of `steps`, every refusal (the one-adjudicator-last refusal permanent, the repeated-kind refusal interim), the new `validate_depth` rule, the load-unchanged check over the repo's workflow and every scaffold template set, the dedup of `agent_ids`/`full_roster`/`gating_agent_ids` (first-step order, full panel then light) and the naming-once behaviour of `reviewers_of` through them | 7 |
| `review_step_role`, `review_step_of`, `is_adjudicator` through them, minting the tokens into criterion 6's loop and `step_roster` (the tokens only, not the roster's shape), lifting criterion 7's repeated-kind refusal | 8 |
| metrics, dash, `workflows/rigger.js` classification | 9 |
| `wave::critic` with `NO_CRITIC_CLAUSE` and the `rigger critique` help wording and the canary | 10 |
| the handbook text | 1 |
| the BUILTIN bodies' language-neutral wording, `REVIEWER_DISCIPLINE`'s toolchain wording | 12 |
| `REVIEWER_DISCIPLINE`'s split, the `## When you review` section, their pins | 13 |
| `BUILTIN`'s `communication` entry, the removal of `RIGGER_COMMUNICATION` and `build_system_prompt`, the handbook's entry list | 14 |
| the two critique prompts' framing, the deletion of the method constants, the `## When you critique a spec or a plan` section in the persona and the template | 15 |

## Global constraints

- Hyphens, never em or en dashes, ASCII only, in every added line.
- A line citation names the symbol at the tree it was read on (1185c8c6, on `rigger-run`); the
  symbol name governs, the line being a pointer only; a `.md` anchor is identified by its quoted
  phrase, its line a pointer at 1185c8c6 only.
- No new event type; no new dependency.
- Every lane `.rigger/gates/lanes.sh` runs green (clippy -D warnings, test), `cargo fmt --check`
  passes on the tree, and the default lane's test suite.
- The planner's batch order is UNIT ORDER's sequence, never checkbox position.
- `.rigger/workflow.yml`, every scaffold gate template set's rendered workflow and every
  `ReviewPanel` fixture in the tree load unchanged and behave as at HEAD. The repo's workflow and
  every scaffold template set name no agent in two roles, which criterion 7's load check confirms;
  the one tree fixture that does (`config_store.rs:1988-1990`, a `tiers.light` panel naming `a` as
  lens and adjudicator) is a config `validate_depth` refuses, so the dedup changes no fixture's
  behaviour.
- No line this spec adds to the `BUILTIN` bodies or `REVIEWER_DISCIPLINE` names a language or a
  toolchain, criterion 12's marker list living in its test; the handbook may name the tools'
  coverage.
- Every operator-facing behaviour named here (the `instructions` output and the shadow advisories,
  the `steps:` key, the thin driver's labels) is pinned by a periphery test as well as a unit test;
  the dash's labels are pinned by `dash.rs` unit tests.

## Done when

- [ ] a test proves THE HANDBOOK STATES THE RULE: `authoring-loops.md` carries S1 and S2 with a five-step ceremony example that, spliced into a workflow, passes `config_store::parse_workflow` and `Config::validate` to five `defaults.review.steps()`. This criterion OWNS the handbook pages OUT OF SCOPE assigns it; `authoring-agents.md` is criteria 2, 3, 4, 12 and 14's, the instructions README criterion 5's and the parse criterion 7's, NOT this one's.
- [ ] a test proves A SAME-NAMED OPERATOR FILE REPLACES A BUILT-IN: `instructions::compose` puts the body of a file named as a `BUILTIN` entry byte for byte into that built-in's `## <name>` section, prints `HEADING` bare, and appends a near name. This criterion OWNS `resolve_layers`' replace arm, `compose`'s replace arm with bare `HEADING`, and `authoring-agents.md:92`; switching off is criterion 3's and `render` criterion 4's, NOT this one's.
- [ ] a test proves AN EMPTY SAME-NAMED OPERATOR FILE SWITCHES A BUILT-IN OFF: `instructions::compose` given an operator file named as a `BUILTIN` entry with an empty or whitespace body injects no section for either, every other section unchanged. This criterion OWNS `resolve_layers`' switched-off arm, its module-doc clause and `authoring-agents.md:85` and `:90`; replacing is criterion 2's, NOT this one's.
- [ ] a test proves EVERY BUILT-IN'S STATE IS REPORTED: `instructions::render`, `shadow_advisories` (printed by `cmd_validate` with a success exit) and `instructions_in_force_line` name each built-in of a named-entry fixture as in force, replaced or switched off through `resolve_layers`. This criterion OWNS the three reporters, their call sites and `authoring-agents.md:94`; `compose` is criteria 2 and 3's, NOT this one's.
- [ ] a test proves THE SCAFFOLDED README DOCUMENTS THE OVERRIDE: `SCAFFOLD_INSTRUCTIONS_README` names every `BUILTIN` entry as `<name>.md` with the replace, switch-off and append rules, and the committed `.rigger/instructions/README.md` equals it (whitespace-normalized text). This criterion OWNS the two README texts and the `BUILTIN`-iterating assertion; the mechanism is criteria 2 and 3's, NOT this one's.
- [ ] a test proves THE MAP FORM IS THE CANONICAL SEQUENCE: `ReviewPanel::steps` returns lenses, adversary, adjudicator for the map form, and `review_unit` over `steps()` gives each CANONICAL IDENTITY SURFACE fixture its prior list with rosters. This criterion OWNS `ReviewStep`, `steps`, its accessors in today's shape, the one loop and `step_roster`'s shape with its wrappers; `steps:` and the dedup are 7's, token spelling 8's, NOT this one's.
- [ ] a test proves THE LIST FORM DECLARES ANY SEQUENCE: `Config::validate` accepts `review: { steps: [..] }` with one adjudicator step at most, last, under every scope, refuses each PARSE AND REFUSALS panel naming its scope, and loads the repo's and each scaffold set's workflow unchanged. This criterion OWNS `ReviewStepDecl`, `steps`, every refusal, that load check and the dedup with its readers; running a sequence is 8's, NOT this one's.
- [ ] a test proves A LIST-FORM SEQUENCE RUNS: each step spawns under its `review_step_role` token, parsed back by `review_step_of` and minted into `step_roster`, the one adjudicator step last reading its verdict as at HEAD. This criterion OWNS the token functions, `is_adjudicator` through them, the minting into the loop and lifting the repeated-kind refusal; roster shape is criterion 6's, refusals 7's, readers 9 and 10's, NOT this one's.
- [ ] a test proves TOKEN READERS CLASSIFY EVERY STEP BY ITS KIND: the readers READERS assigns to criterion 9 (`metrics::review_tier`, `actor_tier`, the adversary-only fold, `dash::stage_of_role`, `role_and_agent`, the `workflows/rigger.js` lookups) read an `@k` token as its kind, and bare tokens fold as today. This criterion OWNS those readers; the token grammar is criterion 8's, NOT this one's.
- [ ] a test proves PANEL READERS READ EVERY STEP: `wave::critic` (with `NO_CRITIC_CLAUSE` and the critique help naming an adversary step in `defaults.review`) and the canary (`apply_model_pins`, `run_canary`) read every step as READERS decides. This criterion OWNS those readers' list-form clauses and the critique skill sentence (`docs.rs:553`); `steps()` and its accessors are criteria 6 and 7's, NOT this one's.
- [ ] `.rigger/gates/lanes.sh` runs green for both feature lanes on the finished tree, verified by `sh .rigger/gates/lanes.sh no-default` and `sh .rigger/gates/lanes.sh core` each exiting 0. This criterion OWNS only the lanes; every behaviour is criteria 1 to 10 and 12 to 15's, NOT this one's.
- [ ] a test proves THE BUILT-IN LAYER NAMES NO LANGUAGE: every `BUILTIN` body and `REVIEWER_DISCIPLINE` have no hit under the MARKERS rule once reworded as BASE decides. This criterion OWNS the bodies' wording, `REVIEWER_DISCIPLINE`'s toolchain words, every existing test that pins a reworded phrase and `docs/handbook/authoring-agents.md:87-88`; the override mechanism is criteria 2 to 5's and the handbook rule criterion 1's, NOT this one's.
- [ ] a test proves THE REVIEWER PROMPT CARRIES ONLY THE CONTRACT: `REVIEWER_DISCIPLINE` holds the two worktree sentences `guard_review_round_tree` enforces and the build-cache sentence; the three moved ones sit under `## When you review`. This criterion OWNS the constant's split, the heading, the verify bullet's and the gate-evidence copy's cuts and the pins that move; the sentences' wording is criterion 12's, NOT this one's.
- [ ] a test proves THE COMMUNICATION DISCIPLINE IS A SHADOWABLE BUILT-IN: `BUILTIN`'s third entry `communication` carries `RIGGER_COMMUNICATION`'s body verbatim, the constant and `build_system_prompt` are gone, and every spawn's system prompt is `compose`'s output, `## communication` once, the old heading gone. This criterion OWNS the entry, both removals, their pins and the handbook's entry list; the override arms are criteria 2 and 3's, NOT this one's.
- [ ] a test proves THE CRITIQUE METHOD IS THE PERSONA'S: `spec_critique_prompt` and the DAG critique prompt carry only the task framing, the voided-duties clause and the output contract, and the method Design moves stands in the adversary and adjudicator personas and their scaffold templates under `## When you critique a spec or a plan`. This criterion OWNS both prompts, the moved text and its pins; the critic's spawn is criterion 10's, NOT this one's.
