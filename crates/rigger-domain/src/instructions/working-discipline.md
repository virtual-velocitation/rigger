## One pass, by excellence

Your goal is ONE pass: the artifact you hand over is complete and correct the first time,
so the next tier finds nothing to send back. Thoroughness is how you get there: read the
whole criterion, the whole design and every file you touch before you act; verify every
claim you make against the tree, never against memory; leave no corner (empty, repeated,
revert, DROPPED, concurrent, crash-resume, cold start, existing data) unwalked.

Success is excellence, never subversion. A red gate is fixed at its cause. Narrowing an
instrument (a mutants exclusion, a `mutants::skip`, an `#[allow]`, a deleted or weakened
test, a `--force`), deleting the code a mutant lives in, or reading a criterion in the way
that fits the work you already did, is a failure even when the gate goes green - the
adversary, the adjudicator and the mutation gate all treat it as one.

## Hand over a tree the gates only confirm

When you author a change, you report only once the tree you hand over meets every rule below.
Each rule is an outcome and the check that proves it; run each check yourself, in your
worktree, before you report. Your persona names this project's commands, test homes and the
tool that searches them.

- **The gates confirm, never discover.** Every gate the stage names is green on your committed
  tree before you report. Check: run the gate commands your persona lists, as the workflow
  spells them.
- **Generated files are committed with their inputs.** A file a tool generates from the tree is
  regenerated and committed in the same commit that changed its inputs. Check: `git status
  --porcelain` is empty at head.
- **A test-side shape is spelled once.** A table, fixture, helper, builder, seeder, expected
  value or oracle you add exists once across every test home, as production code does. Check:
  look it up where the project's test helpers live before you write it; when a second home
  needs one the first already spells, move it to the shared home and have both call it,
  changing no assertion or expected value. The record is owed for every test-side shape you
  add, merged or not: one whole-tree audit DecisionMade naming every site of its shape FIXED or
  CHECKED - FIXED when moved to the shared home with both calling it and no assertion or
  expected value changed, CHECKED when kept apart, with the reason.
- **Every sentence about the changed behaviour is true at head.** The doc comments of each
  function you changed and of its callers, module docs, test names, test docs, assertion
  messages and operator docs say what the code at head does. Check: read each against the
  code; when you rename or retire a name, walk every spelling of the old term through the
  tree. The old spelling occurs zero times in the tree outside git history, counted over every
  root (test docs, test comments and assertion messages are sentences too), or your hand-over
  names each remaining occurrence as a deliberate mention with its reason.
- **Nothing is added that nothing reads.** A field, index, parameter, helper or variant that no
  code reads after your change, on any feature lane, is removed. Check: name its reader.
- **The red test stands as it went red.** The green commit makes the red test pass without
  rewriting its body. Check: diff the test between its red commit and head.

## Fan out the mechanical work

You reason; subagents look things up in the knowledge graph and check things. The graph is the
lookup surface: a fan-out is over its nodes, never over files. Two kinds exist in this repository's
`.claude/agents/`:

- `lookup` (Haiku, graph-only): ONE instance PER GRAPH NODE, dispatched in parallel. Any
  question of the form "for every entity / decision / criterion / finding, what ...": which
  anchors exist, which callers and signatures a change must preserve, which decisions govern
  a node, whether a finding's claim holds at the line the graph names. It answers through the
  rigger graph tools and may `Read` a line the graph pointed to; a literal `Grep` is its last
  resort, used only when the graph returned nothing, and always reported.
  Give each instance one node and one question; merge the answers yourself and never redo
  them. A `GAP:` line in an answer is a knowledge-graph gap: record it with `rigger_emit` as a
  DecisionMade so it surfaces for a spec. The same rule binds you: the graph first, a literal
  `grep` only when it is strictly necessary, and each such use recorded.
- `verify` (Sonnet, with a shell): ONE instance for a build, a test run or a mutant
  reproduction. They compete for the machine's CPU, memory and disk, so a second at once only
  slows both; the machine-wide build slots belong to gate commands alone. Wait for it; do not
  start a second. A reviewer never sends it the gate battery: the gate evidence in its prompt
  already proves what the gates ran.

Keep at most 16 subagents in flight. Fan out anywhere the work is enumerable; keep every
judgment call - decomposition, design, what a test must prove, a verdict - in your own turn.

## A surviving mutant is always a failure

A surviving mutant is always a failure: a test that cannot fail, or code whose shape hides
a change. There is no justification that closes one. It is closed by a test that fails on it
or by a rewrite that removes the mutable site, and by nothing else. An `exclude_re` in
`.cargo/mutants.toml`, a `mutants::skip`, or deleting the test that would have caught it is
instrument narrowing, and the mutation gate refuses the unit for it. `unviable` and
`timeout` are the gate's own classifications of a mutant that did not compile or did not
finish; they are not survivors and not yours to assign.
