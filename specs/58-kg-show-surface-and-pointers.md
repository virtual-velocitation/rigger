# 58 - Close the grep gap: a line-precise show surface and sharper grounding pointers

**Goal:** the knowledge graph is THE way agents look up information, and every grep fallback is a
failure signal. The A/B workload's fallback ledger, classified, found the one lookup class where
agents still reach for grep over the graph: QUOTING CODE - "show me the definition of X, with its
line numbers and body" - because `rigger graph --around` answers structure (what connects to X) but
not text (what X says), so grep is currently the better tool for verifying a claim against the
source. Close that gap: the graph gains a line-precise SHOW surface that prints an entity's
definition site and body from its recorded location, and the grounding pointers every agent receives
name it - so for definitions, callers, peers, AND source text, the graph answers and grep is never
the better tool. (Process greps - filtering one's own gate logs or git output - remain legitimate
and out of scope.)

## Design

### `rigger graph --show <entity>` (the text half of lookup)

- Resolves `<entity>` the way the graph's surfaces already do: a full node id
  (`<file>::<name>`), or a bare name resolved via the pinned name-suffix match - a single
  candidate prints; multiple candidates list (sorted, with files) for the caller to pick; the
  honesty rule is the call views' (never guess among candidates).
- Prints the definition SITE (`file:line`) from the entity's recorded location and the definition
  BODY read from the working tree at that location, line-numbered, bounded (the definition's extent
  when the symbol index knows it, else a bounded window), plus the entity's kind and its one-hop
  degree so the reader knows what they are looking at. Missing file or drifted line degrades to the
  site plus a stale-location note, never an error.
- Read-only, project-scoped, deterministic output ordering; serves from the same projection every
  graph surface reads.

### Sharper grounding pointers (the habit half)

- The grounding tool pointer - carried by EVERY spawn's grounding, the implement slice and the
  review context alike (the fallback ledger shows reviewers grep too) - now names the THREE lookup
  verbs with their jobs on one line: `rigger graph --around` (structure: callers, callees,
  neighbors), `rigger graph --show` (text: definition site and body), `rigger peers` (memory:
  decisions, findings, lessons). One sentence, task-oriented, so reaching for the graph is the
  path of least resistance.
- The `using-rigger` skill and handbook render (the shared discipline body) gain the same
  three-verb lookup guidance, stating the rule plainly: the graph is the lookup surface; grep on
  the project's sources is a fallback worth reporting, not a habit. The docs-drift gate keeps the
  render honest.

### Self-reported fallbacks (measure the gap in the product, not in operator tooling)

- The pointer and the skill instruct: an agent that resorts to grep over the PROJECT'S SOURCES
  because the graph could not answer records ONE line before moving on -
  `rigger progress <id> 'grep-fallback: <what was needed that the graph did not answer>'` - so
  every fallback lands IN THE EVENT LOG with its reason attached. (Filtering one's own build or
  gate output is not a fallback and is not reported.)
- The metrics projection counts `grep-fallback:` progress lines per run and surfaces the count in
  the dash's review-outcomes panel, so the fallback rate is visible run-over-run to every
  consumer - the standing signal that the graph is (or is not yet) the whole lookup surface, and
  a ready-classified worklist of the gaps to close next.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features` (the show surface degrades
  gracefully when the symbol extent is unavailable in a lane; it never errors on a missing
  capability).
- Read-only over the projection and the working tree; no event type, no store write.
- The honesty rules hold: multi-candidate resolution lists, never guesses; drifted locations are
  labeled stale, never silently wrong.
- Deterministic output for a given tree and graph (sorted candidates, stable formatting).

## Done when

- [ ] a test proves SHOW BY ID AND BY NAME: `rigger graph --show` prints the site and
  line-numbered body for a full entity id, resolves a unique bare name to the same, and LISTS
  candidates (sorted, with files) for an ambiguous name without printing any body. This criterion
  OWNS the show surface's resolution and output.
- [ ] a test proves GRACEFUL STALENESS: an entity whose recorded location no longer matches the
  working tree (file missing or line drifted) yields the site plus a stale note - never an error,
  never wrong text presented as current. This criterion OWNS the degrade path.
- [ ] a test proves the POINTER NAMES ALL THREE VERBS: the grounding slice's tool pointer carries
  `--around`, `--show`, and `peers` with their one-line jobs, and the rendered skill/handbook carry
  the same three-verb lookup guidance including the fallback-reporting instruction (drift gate
  green). This criterion OWNS the habit half.
- [ ] a test proves FALLBACKS ARE COUNTED: `grep-fallback:` progress lines recorded during a run
  are counted by the metrics projection and carried in the dash's review-outcomes data, per run.
  This criterion OWNS the in-product measurement; it does NOT own the pointer (the prior
  criterion).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
