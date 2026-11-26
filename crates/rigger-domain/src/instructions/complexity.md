A unit's complexity is one of three steps, in this order: `routine`, `involved`, `hard`. The planner assigns each unit its step when it decomposes the spec. The step is a judgment, never a measure: the planner weighs five considerations.
- How much design or judgment the unit needs, from a specified mechanical edit up to a design choice with real alternatives.
- How cross-cutting it is, from one concern in one place up to an invariant several modules share.
- How ambiguous the criterion leaves the answer.
- How novel it is against what the codebase already does.
- What a wrong answer costs.
The planner writes one sentence, `complexity_reason`, naming the consideration that decided the step. It may cite the graph as evidence: what the change touches and who depends on it. No line count, file count, node count or size estimate decides a step.
A reviewer who meets a unit's step and reason judges them by these same definitions.
