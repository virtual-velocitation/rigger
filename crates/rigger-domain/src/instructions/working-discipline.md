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

## Your turn's end is your session's end

You run as one headless session. When you end your turn the process exits and nothing
re-invokes you: a job you start in the background and any watch you arm die with the
process, and the notification you expect from either never arrives. So you wait for every
job inside your turn - a foreground command, or an `until`-loop that polls its output - and
read its result before you write a word of your report. A command the host moves to the
background when it reaches its timeout is such a job: poll it until it has exited. Check:
when you end your turn, no job you started is running and every claim in your report names
the output it rests on.

A denied or blocked command comes back to you as an error that names its alternative; your
turn is not over - use the alternative.

## Hand over a tree the gates only confirm

When you author a change, you report only once the tree you hand over meets every rule below.
Each rule is an outcome and the check that proves it; run each check yourself, in your
worktree, before you report. Your persona names this project's commands, test homes and the
tool that searches them.

- **The gates confirm, never discover.** Every gate the stage names is green on your committed
  tree before you report. Check: run the gate commands your persona lists, as the workflow
  spells them, each run to completion inside your turn, and read each exit status.
- **Generated files are committed with their inputs.** A file a tool generates from the tree is
  regenerated and committed in the same commit that changed its inputs. Check: `git status
  --porcelain` is empty at head.
- **A test-side shape is spelled once.** A table, fixture, helper, builder, seeder, expected
  value or oracle you add exists once across every test home, as production code does; the
  list is open - a shape is any test-side token two tests could spell, a const or id literal, a
  hand-built event or fixture row, a closure or a query string among them. Naming a shape adds
  it: a builder, fixture or helper that gives an existing inline spelling a name exists once
  across every test home afterwards - every site that already spelled it inline calls it, the
  shared home itself and any file you edit included. Check: look it up where the project's
  test helpers live before you write it; when a second home needs one the first already
  spells, move it to the shared home and have both call it, changing no assertion or expected
  value. The search is an instrument, not a claim: its unit is the statement, never the
  body's name. Walk every test-side body you add or edit - a test fn, a helper, a fixture
  builder, a table row's closure - statement by statement; each statement that is not a single
  call into the shared test home is a shape of its own, and its distinguishing expressions are
  mechanical: every string literal it spells, in full (an assertion, expect or panic message, a
  fixture path, a query text), and every call it makes outside the shared home with its first
  argument token (`create_dir_all(root.join(`, `for_extension("rs")`). Write each down and run
  one search for each over every test home; a hit in another home - an existing helper's body
  or another test's inline statement - is a twin, lifted to the shared home and called from
  both. The record is owed for every test-side shape you add, merged or not: one whole-tree
  audit DecisionMade naming every site of its shape FIXED or CHECKED, carrying each search's
  command, its hit count and every hit - FIXED when moved to the shared home with both calling
  it and no assertion or expected value changed, CHECKED when kept apart, with the reason: the
  named way its shape differs. A CHECKED hit whose shape does not differ is a twin. A search
  you did not run is a record you cannot write. A record whose expressions are names only - a
  fn name, the callee a helper wraps - for a body that spells a literal or an outside call is a
  record you cannot write either. The SDET lens re-runs each recorded search on the review
  tree: a hit the record does not carry, or a CHECKED hit whose reason does not hold, is a
  BLOCKING finding on its own. The lens derives the expression set itself from the unit's test
  hunks at head - every string literal and every outside call with its first argument in each
  test-side body the unit adds or edits - and runs each over every test home: a hit the record
  does not carry, or a record whose expressions are names only for a body that spells either,
  is a BLOCKING finding on its own.
- **Every sentence about the changed behaviour is true at head.** The doc comments of each item
  you changed and of its callers, module docs, test names, test docs, assertion messages and
  operator docs say what the code at head does. Check: enumerate the walked set, each member
  from what generates it - the `//!` module doc of every file the change touches (the
  changed-file list); the doc comment and every comment in the body of each item the change
  touches (the hunks); the doc of the trait item a changed fn or any caller of one implements
  and of the trait a changed impl is for (the impl headers); the doc of every item a changed
  hunk names, callee, constant or type returned (the hunk's identifiers); the doc of every
  caller of a changed item, read from the project's graph at depth 1 (`rigger graph --around
  <file>::<item> --depth 1`, the lines naming callers; the default depth returns the item's
  whole file neighbourhood and is not the caller set), or your persona's fallback; and every
  tracked line outside the hunks that spells a changed item's name, counted over every root as
  an old spelling is below - and read each against the code at head, leaving it true or fixing it;
  when you rename or retire a name, walk every spelling of the old term through the tree. The
  old spelling occurs zero times in the tree outside git history, counted over every root (test
  docs, test comments and assertion messages are sentences too), or your hand-over names each
  remaining occurrence as a deliberate mention with its reason. The record is owed on every
  change: one whole-tree DecisionMade naming the set walked by its generators (files, items,
  trait items, callers, lines counted) and every sentence FIXED with its location, each
  sentence the change writes marked new with its location; a member whose generator returned
  nothing is named as empty. The walk's range is the unit's whole range, the merge base with
  the run branch to head, on every round, never a round's delta alone: a sentence you wrote in
  an earlier round is a member again whenever a later round changes the behaviour it states,
  so a record, or a round's addendum, whose sets are re-derived over a round's delta is a
  record you cannot write. Every sentence the record marks new or FIXED carries its witness,
  the `file:line` of the code at head that makes it true - the arm, the guard, the call, the
  constant; two witnesses when one would not make the whole sentence true - and a sentence
  with no witness is a claim, not a record; a CHECKED sentence whose truth rests on code names
  its witness the same way, and one CHECKED as a different rule sharing the words names that
  rule instead. A rule the change rewrites - a condition
  the code decides by, such as what a call lands on or what a tier means - has spellings no
  generator above reaches: prose that states it without naming a changed item. So every
  spelling of each rewritten rule is walked too, and its search is an instrument, not a claim:
  write down the distinguishing phrases of the rule's OLD statement and of the labels its cases
  carried - the words a sentence stating it cannot be written without, and each name the docs,
  test names and comments give a case the old rule decided (a "cross-file caller" for the calls
  only the old rule sent across files); two phrases when one would miss a spelling - and run
  one recorded search for each over every tracked text line: code, doc comments, body
  comments, test names and docs, assertion messages, specs and operator docs. The record
  carries each search's command, its hit count and every hit, each FIXED, or CHECKED with the
  named way it is not a spelling of the retired rule (a spec's account of the pre-change
  state, a history note, a different rule sharing the words). A rewritten rule with no search
  is a record you cannot write; a change that rewrites no rule names the set empty. A rule is
  rewritten whenever a condition of it changes - a boolean term your production hunks add to or
  remove from a filter, guard, match-arm guard or predicate a function applies - even when no
  sentence of yours moves, and its old statement is what the function's consumers spell: the
  notes they print, their Returns paragraphs, the docs of the variants the answer maps to and
  the oracles that assert them, read from the function's callers at depth 1 in the project's
  graph and their callers' output sentences. For each changed condition write down the rule it
  changes - the question the function answers - and the distinguishing phrases of the old answer
  as the consumers spell them, and run the rule search for each: every hit is FIXED, carrying the
  new qualifier, or CHECKED with the named reason the qualifier does not reach it (a call that
  passes no path, say). The SDET
  lens re-derives each generator's set on the review tree from the diff and the graph - the
  changed-file list; each hunk's identifiers, doc links (`[...]`) and constants included; the
  body comments of each changed item; the impl headers; the depth-1 callers; the tracked lines
  spelling a changed item's name - compares it with the walk record, and re-runs each recorded
  rule-spelling search and re-reads every hit, CHECKED ones included, against the code at head:
  a member the record omits, a hit the record does not carry, or a CHECKED hit whose reason does
  not hold at head, is a BLOCKING finding on its own. The lens derives every set over the
  unit's whole range, as the record does, never the round's delta its prompt carries, and
  reads every witness the record carries against the code at head: a sentence whose witness no
  longer makes it true, a new or FIXED sentence with no witness, or a record whose sets are
  re-derived over a round's delta, is a BLOCKING finding on its own. The lens derives the
  changed-condition set itself from the unit's production hunks over the whole range - every
  added or removed boolean term inside a filter, guard or predicate - names the rule each
  changes, and reads every consumer sentence of that function - its depth-1 callers' notes,
  Returns paragraphs, variant docs and oracles - against the condition at head: a consumer
  sentence stating the pre-change answer that the record does not carry FIXED or CHECKED is a
  BLOCKING finding on its own.
- **Nothing is added that nothing reads.** A field, index, parameter, helper or variant that no
  code reads after your change, on any feature lane, is removed. Check: name its reader.
- **The red test stands as it went red.** The green commit makes the red test pass without
  rewriting its body. Check: diff the test between its red commit and head.
- **Every mutable token of your production delta is pinned.** Every token of the production
  code you add or change whose mutation would leave the behaviour wrong turns some test red when
  it is mutated; the list is open: a comparison operator (flip its bound), an arithmetic offset
  (drop the +1 or -1), a boolean operand (negate or remove it), a call argument that is a range,
  index or bound (degenerate it), a statement in one arm of a branch (delete it), a match arm
  (swap it with a sibling), an early return or continue (remove it). Check: enumerate the tokens
  from your production hunks over the unit's whole range, the merge base with the run branch to
  head; apply each mutation in your worktree one at a time, run the crate's tests and the
  touched test binaries, read the red, and restore the tree - a probe is over before your turn
  is, as the rule below says. The record is one whole-tree `<unit>-mutant-probes` DecisionMade
  carrying, for each token, its `file:line`, the mutation applied, and the test that went red
  with its assertion. A token whose probe leaves every test green is pinned by a new test before
  you report, or the code is reshaped so the token disappears (one `span.contains(&x)` in place
  of two comparisons, say) and the reshaped form probed. A token with no recorded probe is a
  record you cannot write. The enumeration is by hand and stands once the mutation gate runs
  too, since the mutation gate's tool mutates no call argument and deletes no single statement
  of an arm.
  The SDET lens derives the token set itself from the production hunks over the unit's whole
  range and probes every token the record lacks: a probe that leaves every test green, or a
  token with no recorded probe, is a BLOCKING finding on its own.
- **A probe that edits your tree is over before your turn is.** A reversion probe, or any check
  that edits your worktree, runs to completion and restores the tree before your turn ends,
  never as a detached or background job: the attempt checkpoint commits whatever the worktree
  holds when your session ends. It is the sharpest case of the rule that your turn's end is
  your session's end, since the job the exit cuts short leaves your tree half-edited. Check:
  no job you started is still running and `git status --porcelain` is empty when you end your
  turn.

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
