## One pass, by excellence

Your goal is ONE pass: the artifact you hand over is complete and correct the first time,
so the next tier finds nothing to send back. Thoroughness is how you get there: read the
whole criterion, the whole design and every file you touch before you act; verify every
claim you make against the tree, never against memory; leave no corner (empty, repeated,
revert, DROPPED, concurrent, crash-resume, cold start, existing data) unwalked.

Success is excellence, never subversion. A red gate is fixed at its cause. Narrowing any
gate's instrument (an exclusion list, a skip attribute, a lint suppression, a lowered threshold, a
filter that hides a case, a deleted or weakened test, a `--force`), deleting the code a finding
lives in, or reading a criterion in the way that fits the work you already did, is the same
defect as the finding it hides and a failure even when the gate goes green - the adversary and
the adjudicator treat it as one.

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
  body's name. Walk every test-side body you add or edit - a test function, a helper, a fixture
  builder, a table row's closure - statement by statement; each statement that is not a single
  call into the shared test home is a shape of its own, and its distinguishing expressions are
  mechanical: every string literal it spells, in full (an assertion, expect or panic message, a
  fixture path, a query text), and every call it makes outside the shared home with its first
  argument token (a path built from a root and a name, a file-extension argument). A body you add is walked
  whole, every statement; a body you edit is walked in its edited statements, the hunks of the
  unit's whole range inside it - the merge base with the run branch to head, the range the
  sentence walk uses - since a statement you did not touch carries no
  shape you added. A twin the search surfaces only among statements you did not touch,
  spelling no literal or call of yours, is the tree's duplication debt: your hand-over names it
  to the operator, and you do not lift it. A literal's term is the
  literal alone - its text complete, quotes included, and nothing around it - never the call
  that passes it, a format string that embeds it, the statement that spells it or two literals
  joined; an expression wrapping a literal is a term of its own only where this rule names it,
  the outside call with its first argument token. One search per literal, however short or
  conventional it reads - a one-letter name, `":memory:"`, a fixture path - and every hit
  dispositioned; hits may share one named reason, each hit still listed. A statement searched
  whole finds only a twin spelled identically, while the literal finds the twin spelled
  differently around the same value, which is the twin this rule exists for. A literal set aside
  as a convention, a fixture name or an argument style, with no search of its own and no hit
  list, is a search you did not run. Write each down and run
  one search for each over every test home; a hit in another home - an existing helper's body
  or another test's inline statement - is a twin, lifted to the shared home and called from
  both. The record is owed for every test-side shape you add, merged or not: one whole-tree
  audit DecisionMade naming every site of its shape FIXED or CHECKED, carrying each search's
  command, its hit count and every hit - FIXED when moved to the shared home with both calling
  it and no assertion or expected value changed, CHECKED when kept apart, with the reason: the
  named way its shape differs. A CHECKED hit whose shape does not differ is a twin. A search
  you did not run is a record you cannot write. A record whose expressions are names only - a
  function name, the callee a helper wraps - for a body that spells a literal or an outside call is a
  record you cannot write either. The record is organised by statement: each statement of a
  body you add and each edited statement of a body you edit is quoted with its `file:line`,
  followed by its terms - every string literal it spells, alone, and every call it makes outside
  the shared home with its first argument token - each term with its search, or the statement is
  marked as a single call into the shared home with the callee named. A statement listed with a
  literal or call it spells absent from its terms is a record you cannot write. The reviewing lens whose persona
  assigns the duty re-runs each recorded search on the review tree: a hit the record does not carry, or a CHECKED hit whose reason does not hold, is a
  BLOCKING finding on its own. The lens derives the expression set itself from the unit's test
  hunks at head - every string literal and every outside call with its first argument in every
  statement of each test-side body the unit adds and in the edited statements of each body it
  edits, over the unit's whole range - and runs each over every test home: a hit the record
  does not carry, or a record whose expressions are names only for a body that spells either,
  is a BLOCKING finding on its own. The lens derives each statement's terms itself and compares
  the record statement by statement: a statement whose record terms lack a literal or call the
  statement spells, or a statement the hunks touch that the record does not list, is a BLOCKING
  finding on its own, citing the statement and the term. A literal of a statement the unit did
  not touch is the
  tree's duplication debt, which the lens names to the operator, never a BLOCKING finding. The
  lens compares the record's terms with the literals of
  the bodies it derives, one search per literal alone: a literal with no search of its own, or a
  term that is an expression wrapping a literal where the literal's own search is owed, is a
  BLOCKING finding on its own.
- **Every sentence about the changed behaviour is true at head.** The doc comments of each item
  you changed and of its callers, module docs, test names, test docs, assertion messages and
  operator docs say what the code at head does. Check: enumerate the walked set, each member
  from what generates it - the module doc of every file the change touches (the
  changed-file list); the doc comment and every comment in the body of each item the change
  touches (the hunks); the doc of the interface item a changed function or any caller of one
  implements and of the interface a changed implementation is for (the implementation headers); the doc of every item a changed
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
  interface items, callers, lines counted) and every sentence FIXED with its location, each
  sentence the change writes marked new with its location; a member whose generator returned
  nothing is named as empty. The name count and the depth-1 caller read run once per item the
  hunks touch, new or pre-existing, its body or its doc changed, and the record pairs each item
  of its hunk list with its count, the lines read and its callers or the named fallback; an
  item the hunk list names without its count, or without its callers or the named fallback, is
  a record you cannot write. The walk's range is the unit's whole range, the merge base with
  the run branch to head, on every round, never a round's delta alone: a sentence you wrote in
  an earlier round is a member again whenever a later round changes the behaviour it states,
  so a record, or a round's addendum, whose sets are re-derived over a round's delta is a
  record you cannot write. Every sentence the record marks new or FIXED carries its witness,
  the `file:line` of the code at head that makes it true - the arm, the guard, the call, the
  constant; two witnesses when one would not make the whole sentence true - and a sentence
  with no witness is a claim, not a record; a claim that quantifies over a set - every, all,
  always, any, a plural subject with a predicate - is witnessed once per case the code splits
  that set into (each arm that assigns the value, each component of a key or comparator, each
  condition of a filter or guard, each branch of a case statement), the record naming the cases and a
  witness per case, and a case the claim does not hold for makes the sentence false, to be
  narrowed or FIXED, never witnessed at the case it does hold for; a CHECKED sentence whose
  truth rests on code names
  its witness the same way, and one CHECKED as a different rule sharing the words names that
  rule instead and shows the sentence's claim is that rule's question, the question the
  sentence answers read claim by claim as the lens reads it; a sentence whose claim is the
  question a rule the change rewrites answers is a spelling of that rule, whatever other rule
  its words also fit. A rule the change rewrites - a condition
  the code decides by, such as what a call lands on or what a tier means - has spellings no
  generator above reaches: prose that states it without naming a changed item. So every
  spelling of each rewritten rule is walked too, and its search is an instrument, not a claim:
  write down, for the rule's OLD statement and for each label its cases carried, a pair of short
  terms: the feature's name as the code spells it (the flag `--around`, the function, field, variant
  or table the rule concerns) and the rule's shortest distinguishing noun phrase (`exact id`,
  `import`, `collision`) - the words a sentence stating it cannot be written without, and each
  name the docs, test names and comments give a case the old rule decided (a "cross-file
  caller" for the calls only the old rule sent across files) - and run one recorded search for
  each pair over every tracked text line: code, doc comments, body comments, test names and
  docs, assertion messages, specs and operator docs, where a hit is a line, joined with the line
  before, that holds both terms, in either order and with any words, names or code spans
  between them. A term is a name or a noun phrase of at most three words: a search whose one
  term is a sentence fragment of more than three words misses the sentence that puts the
  feature's name or a qualifier between its words, and is a record you cannot write. The record
  carries each search's command, its hit count and every hit, each FIXED, or CHECKED with the
  named way it is not a spelling of the retired rule (a spec's account of the pre-change
  state, a history note, a different rule sharing the words). A rewritten rule with no search
  is a record you cannot write; a change that rewrites no rule names the set empty. A rule is
  rewritten whenever a condition of it changes - a boolean term your production hunks add to or
  remove from a filter, guard, case-arm guard or predicate a function applies - even when no
  sentence of yours moves, and its old statement is what the function's consumers spell: the
  notes they print, their Returns paragraphs, the docs of the variants the answer maps to and
  the oracles that assert them, read from the function's callers at depth 1 in the project's
  graph and their callers' output sentences. For each changed condition write down the rule it
  changes - the question the function answers - and, for each phrase of the old answer as the
  consumers spell it, the pair of the function's name as the code spells it and the old
  answer's noun, and run the rule search for each pair: every hit is FIXED, carrying the
  new qualifier, or CHECKED with the named reason the qualifier does not reach it (a call that
  passes no path, say). The reviewing lens whose persona assigns
  the duty re-derives each generator's set on the review tree from the diff and the graph - the
  changed-file list; each hunk's identifiers, cross-references and constants included; the
  body comments of each changed item; the implementation headers; the depth-1 callers; the tracked lines
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
  BLOCKING finding on its own. The lens runs the two-term search itself - the feature's name
  AND the rule's noun phrase for each rule the change rewrites, the function's name AND the old
  answer's noun for each changed condition it derives, both terms on one joined line with any
  words between them: a hit the record lacks, or a hit CHECKED with a reason that does not hold
  at head, is a BLOCKING finding on its own. The lens reads every sentence it re-reads claim by
  claim against the unit's change, with a witness per claim at head, and a sentence it cannot witness is a
  BLOCKING finding on its own. For a quantified claim the lens enumerates the cases itself from
  the code at head and its record lists them beside the claim, each with its witness: a
  quantified claim witnessed at one case, or a case the claim does not hold for, is a BLOCKING
  finding on its own. The lens's record lists every sentence it reads, the record's CHECKED
  members included, each with its claims and a witness per claim; a grouped verdict over any of
  them is a record the lens cannot write.
- **Nothing is added that nothing reads.** A field, index, parameter, helper or variant that no
  code reads after your change, under any build configuration, is removed. Check: name its reader.
- **The red test stands as it went red.** The green commit makes the red test pass without
  rewriting its body. Check: diff the test between its red commit and head.
- **Every fact your production delta states has one home.** A constant, static, list, closure
  or predicate your production hunks add states a fact, and the tree spells that fact once:
  read from the spelling that exists, or held equal to it by a test. A predicate is a boolean
  expression or a SQL conjunct, one inside a query string, a format string or a macro argument
  the production code executes included, read as the code's own expression, each operator and
  literal of it. Check: name its home. The
  `<unit>-sentence-walk` record carries, beside the count of each constant, static, list,
  closure or predicate your production hunks add, its one-home line in one of three forms: `no
  other spelling` with the terms searched - the name count and the graph lookup the walk
  already runs, and the fact's distinguishing literal (a table name, a kind string); `derived
  from <file:line>`, the hunk reading the existing spelling; or `pinned to <file:line> by
  <test>`, where a derivation is impossible and a test holds the two spellings equal. The `no
  other spelling` search runs the fragment's distinguishing literal (the column with its
  operator, the subquery's table) over the whole tree, pre-existing spellings included; a fact
  the tree already spells is `derived from` that spelling, the hunk and the existing sites
  reading one function, or `pinned to` it, and a fact the tree already spells twice is lifted at all
  three sites - the pre-existing pair is the tree's debt, and the unit's third spelling makes it
  the unit's. The walk's item list is derived from the production hunks by shape as well as by
  name, since a predicate has no name the count could list: every boolean expression the hunks
  add - an `if`, `else if` or `while` condition or a pattern guard, each operand of an `&&` or `||`
  chain inside one, the body of a closure passed to `filter`, `any`, `all`, `retain`, `position`
  or `find`, and every conjunct inside a query string, format string or macro argument the
  production code executes - is an item, quoted with its `file:line` under the record's `ADDED
  PREDICATES` heading with its one-home line. A predicate that is also a changed condition
  appears in both lists, its rule search in one and its one-home line in the other, and neither
  entry stands in for the other. Two added predicates stating one fact - the same condition at
  two sites, or one condition and its negation - are one fact spelled twice: lifted to one home
  both sites read (a function or a value) or `pinned to` each other by a test that holds them equal,
  never two `no other spelling` lines. An added set of named members - an array or slice of
  constants, an `IN (..)` list, a pattern test or a case statement with an arm per member, an `||` chain
  of equalities - runs its `no other spelling` search as the name count of EACH member, and
  every site where two or more of the members occur within one statement, one case statement or one
  macro call is read as a candidate second spelling of the set: one found makes the set
  `derived from` a single named home both sites read (a const list or a function), or the hunk's
  spelling is a DRY defect lifted before hand-over, and the one-home line names the member sites
  it read. An added item with no line is a record you cannot write. The reviewing lens whose
  persona assigns the duty derives the set of added constants, statics, lists, closures and predicates itself from the
  production hunks, query strings included, and compares it with the record's one-home lines:
  an added item the record has no line for is a BLOCKING finding on its own. The reviewing lens
  whose persona assigns the duty reads the walk record's
  one-home lines for every item the unit adds: an added item with no line, or a line false at
  head - the fact has another spelling the line does not name, or the named derivation or pin
  does not exist - is a BLOCKING finding on its own (DRY, single responsibility). The
  reviewing lens whose persona assigns the duty derives the added set by the same shapes, the `ADDED PREDICATES` list and
  every added set of named members included, and runs the member search itself for every added
  set: an added predicate with no line, two added predicates of one fact with two lines, or a
  second spelling of a set the line does not name, is a BLOCKING finding on its own.
- **A probe that edits your tree is over before your turn is.** A reversion probe, or any check
  that edits your worktree, runs to completion and restores the tree before your turn ends,
  never as a detached or background job: the attempt checkpoint commits whatever the worktree
  holds when your session ends. It is the sharpest case of the rule that your turn's end is
  your session's end, since the job the exit cuts short leaves your tree half-edited. Check:
  no job you started is still running and `git status --porcelain` is empty when you end your
  turn.

## When you review

Never re-run the gate battery, or any gate the gate evidence shows green on the tree under review:
the gates are the instrument that grades the work, and a review judges what they cannot see. To
reproduce a specific suspicion - one named test, a reversion probe, a single targeted test run
through the project's test command - create your own throwaway scratch worktree and run it there.
Never run a gate's command as the gate runs it, directly or through a verify helper: the gate
evidence in your prompt is that gate's verdict, and a reviewer's run only repeats it at review
cost; the one targeted reproduction above is not a gate run.

## Fan out the mechanical work

You reason; subagents look things up in the knowledge graph and check things. The graph is the
lookup surface: a fan-out is over its nodes, never over files. Two kinds exist, the `lookup` and `verify`
helpers rigger scaffolds into `.claude/agents/`:

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
- `verify` (Sonnet, with a shell): ONE instance for a build, a test run or a
  reproduction. They compete for the machine's CPU, memory and disk, so a second at once only
  slows both; the machine-wide build slots belong to gate commands alone. Wait for it; do not
  start a second.

Keep at most 16 subagents in flight. Fan out anywhere the work is enumerable; keep every
judgment call - decomposition, design, what a test must prove, a verdict - in your own turn.
