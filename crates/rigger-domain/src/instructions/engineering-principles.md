## The engineering principles are the law of this codebase

Clean Architecture governs the shape of the system; SOLID, DRY, KISS, YAGNI and TDD apply to
every change you plan, write, test or review; BDD applies wherever a behavior is operator-facing. No spec, deadline or red gate overrides
them; a change that violates one is a failure even when every test passes.

**Clean Architecture.** Dependencies point inward only: entities and use cases (the domain:
specs, units, runs, decisions, the fold, the review verdict) know nothing of stores, processes,
Claude Code, git, HTTP or the terminal. Ports are traits owned by the inner layer; adapters
(the SQL and segmented stores, the graph projector, the headless host, the git worktree, the
dashboard, the CLI) implement them from outside and are wired in exactly one composition
root. A use case never imports an adapter; an adapter never holds domain rules. Framework
and I/O types stop at the adapter boundary and are translated into domain types there. The
pure `core` lane compiling for wasm with no filesystem, process, network or clock is the
mechanical proof that the inner layer is clean, and every crate's place in the dependency
order below is that rule made structural.

**SOLID.** Single responsibility: one crate per concern, one module per responsibility, one
reason to change per type. Open/closed: extend through the existing ports and enums; a new
behavior is a new adapter or variant, never a branch bolted into a caller.
Liskov: an adapter honors its port's whole contract; the conformance suites pass unchanged.
Interface segregation: ports stay small; a consumer depends on the trait it uses. Dependency
inversion: domain and conductor code depend on traits; stores, processes, clocks and
filesystems are injected at the composition root and nowhere else, and the pure `core` lane
keeps compiling without them. Crate dependencies point one way, pinned by a structural test:
core <- store, graph, grounder <- driver, gates <- conductor <- console, dash <- cli.

**DRY.** Logic that exists in two places anywhere in the tree is a defect, with no size
exemption. Before writing a function, look it up in the graph; extend the one that exists.
The simplification audit's duplication and dead-code reports are gates, not advisories.

**KISS.** The simplest design that meets the criterion: the fewest moving parts and points
of failure. Line counts measure nothing here; never split or merge code to hit a size.

**YAGNI.** Build exactly what the criterion names. No speculative parameter, trait method,
config key, feature flag or "for later" abstraction. Anything the criterion does not require
and no caller uses is removed before review.

**TDD.** The failing test is written first and committed first; the code that makes it pass
comes second, in a separate commit, so the unit's history shows red then green. A test
written after the code to match its output is not TDD and the reviewer treats it as missing.
Every test must also survive mutation testing (see that section).

**BDD, when relevant.** An operator-facing behavior (a CLI verb, a dashboard view, a plugin
skill, a hook) is specified and tested as behavior: given a state, when the operator acts,
then the observable outcome, in a periphery test named for the behavior and exercising the
real surface, never the internals.

When you find a violation you did not cause, record it as a `LessonLearned` naming the file
and the principle; do not extend it.

## One pass, by excellence

Your goal is ONE pass: the artifact you hand over is complete and correct the first time,
so the next tier finds nothing to send back. Thoroughness is how you get there: read the
whole criterion, the whole design and every file you touch before you act; verify every
claim you make against the tree, never against memory; leave no corner (empty, repeated,
revert, concurrent, crash-resume, cold start) unwalked.

Success is excellence, never subversion. A red gate is fixed at its cause. Narrowing an
instrument (a mutants exclusion, a `mutants::skip`, an `#[allow]`, a deleted or weakened
test, a `--force`), deleting the code a mutant lives in, or reading a criterion in the way
that fits the work you already did, is a failure even when the gate goes green - the
adversary, the adjudicator and the mutation gate all treat it as one.

## A surviving mutant is always a failure

A surviving mutant is always a failure: a test that cannot fail, or code whose shape hides
a change. There is no justification that closes one. It is closed by a test that fails on it
or by a rewrite that removes the mutable site, and by nothing else. An `exclude_re` in
`.cargo/mutants.toml`, a `mutants::skip`, or deleting the test that would have caught it is
instrument narrowing, and the mutation gate refuses the unit for it. `unviable` and
`timeout` are the gate's own classifications of a mutant that did not compile or did not
finish; they are not survivors and not yours to assign.
