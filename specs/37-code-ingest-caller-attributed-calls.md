# 37 - The knowledge graph answers "who calls X": caller-attributed CALLS edges

**Goal:** restore the grounding heart to its designed value. The reference architecture (§5) and the
agentic-SDLC handbook promise that an agent GROUNDS ITSELF BY QUERYING the graph - "before writing
code it grounds itself (`rigger ground`, `rigger graph --around <file>`)" - and that the graph is
"the queryable map ... ALL & ONLY its context, not a chunk dump." That promise fails on the single
most common question an agent asks of code - WHO CALLS X - because the code ingest is a LOSSY FOLD:
tree-sitter parses that a reference to `G` occurs inside the body of definition `F`, but the emit
pass lowers every reference to `<file> --REFERENCES--> G`, discarding the caller `F`. So
`rigger graph --around G` can only answer "referenced from <file>", never "called by F", and an agent
debugging real code falls back to a text search - the self-reinforcing memory the whole architecture
exists to provide does not reinforce. This spec attributes each reference to its ENCLOSING definition
and adds a caller-attributed `<file>::F --CALLS--> G` edge, so ONE `rigger graph --around G` answers
"who calls G" by function and the graph finally earns the query the design already routes to it. It
is the highest-leverage corrective and is deliberately SCOPED to the code layer; the sibling drifts
(grounding delivered as a truncated push rather than a query; the loop not enforcing done = merged
PR + branch-GC; the dash not auto-starting on the native `step` path) are each their OWN focused
corrective spec, kept separate so the fix does not repeat the breadth-over-core drift it corrects.
Additive: the existing `<file> --REFERENCES--> G` edge and its consumers are untouched.

## Design

The extractor (`src/grounder/symbols/extract.rs`, the ONLY function that touches tree-sitter) runs
the grammar's `tags` query and lowers each tag into a `Def { kind, name, line }` or a
`SymRef { name, line }` (`src/grounder/symbols/model.rs`). A `SymRef` keeps only name + line - the
enclosing definition is thrown away. The emit pass (`src/grounder/symbols/events.rs`
`extract_events`) turns each `SymRef` into `EdgeInferred { file, name, lang, fresh }`, and the fold
(`src/contextgraph/sqlite.rs`) resolves `name` to a callee entity and folds
`<file> --REFERENCES--> <callee>`.

Each tag already carries a byte RANGE (`tree_sitter_tags::Tag::range`), not just the name span the
extractor keeps. Capture each definition's range and each reference's position, and attribute each
reference to the INNERMOST definition whose range contains it - that enclosing definition is the
caller. (If the tags do not expose a usable definition-body range, the extractor recovers the
enclosing definition from the parsed tree instead; the Done-when contract is mechanism-agnostic.) A
reference not inside any definition - a top-level `use`/import - has no caller and emits only today's
file edge.

Thread the caller through to the fold:
- `SymRef` gains the enclosing definition it was attributed to (the caller def's name, an `Option`),
  set during extraction; a ref outside every definition carries `None`.
- `EdgeInferred` (`src/contextgraph`) gains `caller: Option<String>` - the enclosing definition name
  in the same file. `extract_events` emits it; the existing deterministic refs order is unchanged
  (the caller is derived, never a new sort key).
- The fold, when `caller` is present, ADDS `<file>::<caller> --CALLS--> <callee>` under a new
  `REL_CALLS`, ALONGSIDE the existing `<file> --REFERENCES--> <callee>`. Callee resolution is
  UNCHANGED - the same name-to-entity resolution the REFERENCES edge already uses. Re-extraction
  supersedes the file's CALLS edges with its other structural edges under the same `fresh` batch
  boundary (spec 29a), so a changed file replaces rather than accretes.

`REL_CALLS` is a new structural relation. The existing `REFERENCES` / `CONTAINS` edges, the two-view
blast radius, and the grounder are unchanged; this spec is additive to the code layer only.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`. The emit pass and the fold compile
  in BOTH lanes (the fold stays compiled in the grep-only lane, spec 29a).
- Determinism by construction: the caller attribution and the emitted events are deterministic for
  identical source (byte-identical events); any sorted/serialized set uses `BTreeMap`/`BTreeSet`/
  sorted `Vec`. Deriving the caller must NOT reorder the existing deterministic refs emission.
- Event-sourced: the CALLS edge is a projection - it folds from `EdgeInferred` like every other
  structural edge, and re-extraction SUPERSEDES it (sets `valid_to`) under the file's `fresh` batch
  boundary; nothing is a mutable side index, nothing is deleted.
- Purely ADDITIVE to the code layer: the existing `REFERENCES` / `CONTAINS` edges, the two-view
  blast radius, and the grounder are unchanged. A reference with no enclosing definition still emits
  exactly today's `<file> --REFERENCES--> <symbol>` edge and no CALLS edge.

## Done when

- [ ] a test proves the EXTRACTOR attributes a reference to its enclosing definition: for source
  where `fn F() { G(); }`, the extracted `SymRef` for `G` carries `F` as its enclosing definition,
  and a reference NOT inside any definition (a top-level `use`) carries none. This criterion OWNS the
  enclosing-definition attribution in extraction.
- [ ] a test proves the EMIT pass carries the caller: `extract_events` lowers a reference inside `F`
  to an `EdgeInferred` whose `caller` is `F`, and a top-level reference to one with no caller, with
  identical source yielding byte-identical events. This criterion OWNS the caller field on the
  emitted event; it does NOT own the extraction attribution (criterion 1).
- [ ] a test proves the FOLD adds the CALLS edge: folding an `EdgeInferred` with `caller = F` for a
  reference to `G` in `<file>` yields a `<file>::F --CALLS--> <callee-of-G>` edge, WHILE the existing
  `<file> --REFERENCES--> <callee-of-G>` edge is still produced (additive). This criterion OWNS the
  CALLS-edge fold and `REL_CALLS`; it does NOT own emission (criterion 2).
- [ ] a test proves the ACID TEST end to end: after ingesting a file where `F` and `H` reference `G`
  from inside their bodies and `K` does not, a `subgraph` around `<file>::G` returns
  `<file>::F --CALLS--> G` and `<file>::H --CALLS--> G` and NO CALLS edge from `K` - the graph
  answers "who calls G" by function, the query the design routes to an agent. This criterion OWNS the
  who-calls-X traversal result; it does NOT own the fold (criterion 3).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
