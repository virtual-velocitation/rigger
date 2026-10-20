## The engineering principles are the law of this codebase

Clean Architecture governs the shape of the system; SOLID, DRY, KISS, YAGNI and TDD apply to
every change you plan, write, test or review; BDD applies wherever a behavior is operator-facing. No spec, deadline or red gate overrides
them; a change that violates one is a failure even when every test passes.

**Clean Architecture.** Dependencies point inward only: the domain's entities, use cases and
rules know nothing of stores, processes, the agent host, git, HTTP or the terminal. Ports are
interfaces owned by the inner layer; adapters (a store, a projector, a host, the git worktree,
the dashboard, the CLI) implement them from outside and are wired in exactly one composition
root. A use case never imports an adapter; an adapter never holds domain rules. Framework
and I/O types stop at the adapter boundary and are translated into domain types there. The
inner layer compiling without the outer layers, with no filesystem, process, network or clock,
is the mechanical proof that it is clean.

**SOLID.** Single responsibility: one package per concern, one module per responsibility, one
reason to change per type. Open/closed: extend through the existing ports and enums; a new
behavior is a new adapter or variant, never a branch bolted into a caller.
Liskov: an adapter honors its port's whole contract; a port's conformance tests pass unchanged.
Interface segregation: ports stay small; a consumer depends on the interface it uses. Dependency
inversion: domain and use-case code depend on interfaces; stores, processes, clocks and
filesystems are injected at the composition root and nowhere else, and the inner layer
keeps compiling without them. Package dependencies point one way, pinned by a structural test.

**DRY.** Logic that exists in two places anywhere in the tree is a defect, with no size
exemption. Before writing a function, look it up in the graph; extend the one that exists.

**KISS.** The simplest design that meets the criterion: the fewest moving parts and points
of failure. Line counts measure nothing here; never split or merge code to hit a size.

**YAGNI.** Build exactly what the criterion names. No speculative parameter, interface method,
config key, feature flag or "for later" abstraction. Anything the criterion does not require
and no caller uses is removed before review.

**TDD.** The failing test is written first and committed first; the code that makes it pass
comes second, in a separate commit, so the unit's history shows red then green. A test
written after the code to fit its output is not TDD and the reviewer treats it as missing.

**BDD, when relevant.** An operator-facing behavior (a CLI verb, a dashboard view, a plugin
skill, a hook) is specified and tested as behavior: given a state, when the operator acts,
then the observable outcome, in a periphery test named for the behavior and exercising the
real surface, never the internals.

When you find a violation you did not cause, record it as a `LessonLearned` naming the file
and the principle; do not extend it.
