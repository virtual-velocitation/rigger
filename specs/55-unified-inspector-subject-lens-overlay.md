# 55 - The unified inspector: subject x lens re-projection and the rationale overlay

**Goal:** the inspector's parts - three lenses (files / code / concepts), the seeded neighborhood, the
directed call views, and per-node provenance - become ONE coherent instrument: a SUBJECT (whatever is
selected) times a LENS (the altitude it is viewed at), composing any-to-any, with a RATIONALE overlay
that answers "why is this here" at every altitude. Today each piece works alone; what is missing is
the composition the graph-inspector addendum fixes in its subject-by-lens matrix (section 2) and
rationale overlay (section 4): selecting the knowledge-graph concept and flipping to the Code lens
must show the functions and docs that realize it; flipping to Files must show the literal files those
members resolve to; focusing any node must reveal the decisions, findings, and lessons attached to
it. Every (subject, lens) cell is DEFINED - including the empty and degenerate ones - and the overlay
is progressive disclosure, never clutter.

## Design

### Subject x lens re-projection (`src/dash.rs` route + `src/dash.html`)

- **The route composes `seed` with `lens`:** `seed=<id>&lens=<files|code|concepts>` re-grains the
  SUBJECT at the chosen altitude instead of switching to a whole-graph overview. The projection rule:
  resolve the subject's MEMBER SET at its own grain (a concept's `REALIZES` members; a community's
  members; a file's contained entities; a single entity is its own set), then re-bucket that set
  under the requested lens through the existing overview/drill folds. So concept X under `lens=code`
  shows X's members grouped by coupling community; under `lens=files` it shows the DISTINCT FILES the
  members resolve to.
- **Cross-grain resolution never trusts the id:** projecting members to files resolves a BARE
  placeholder node to its DEFINING file via the pinned name-suffix match (single candidate) - a
  multi-candidate bare node surfaces as a marked unresolved entry, never a wrong attribution (the
  call views' honesty rule, applied to re-projection).
- **Every cell is defined:** a subject with no membership under the target lens returns the
  documented empty-cell state ("not part of any concept" / "no derived communities"); a wide
  re-grain truncates through the existing render budget with its "showing N of M" caption; a
  multi-bucket member appears once, flagged shared. No cell errors; no cell invents.
- **The client keeps subject across lens flips:** the lens selector re-requests the SAME seed under
  the new lens (subject sticky, altitude swapped); clearing the subject returns to that lens's
  whole-graph overview. The existing views (neighborhood, calls, drill) remain reachable from any
  member click - the instrument composes rather than modes.

### The rationale overlay (`src/dash.rs` + `src/dash.html`)

- **Per-node "why" on demand:** every rendered node (any lens, any view) can reveal its rationale
  LEAVES - the decisions, findings, and lessons attached to it through the live knowledge edges
  (`ABOUT` / `GOVERNS` / the finding and lesson attachments), served through the existing per-node
  provenance query, batched per request for the visible nodes that have any.
- **Progressive disclosure:** an overlay TOGGLE in the toolbar; when on, nodes carrying rationale
  show a small count badge; expanding a badge lists each leaf as a one-line summary that expands to
  its full content (the decision-history disclosure pattern the dash already uses). When off,
  nothing renders - zero cost, zero clutter.
- **Content, not machinery:** leaves show the knowledge CONTENT (summary, reasoning); no builder
  personas or run bookkeeping surface (the target-project-only discipline). A node with no rationale
  simply carries no badge; a project with no decisions renders no badges anywhere - graceful
  absence, never an error.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Read-only over the projection; no event type, no store write; served through the lazy graph
  provider, never the state poll.
- Determinism by construction: member sets, buckets, file resolutions, and rationale leaf lists sort
  deterministically; the same graph and parameters yield byte-identical responses.
- Honesty rules carry over: bare-node file attribution only on a single candidate; shared membership
  flagged; truncation captioned; empty cells documented - the view may be incomplete but never
  confidently wrong.
- Additive: with no `seed`+`lens` composition and the overlay off, every existing view is
  byte-identical. Project-scoped throughout.

## Done when

- [ ] a test proves SUBJECT x LENS re-projection: a fixture concept re-grained under `lens=code`
  returns its members bucketed by community, and under `lens=files` returns the distinct defining
  files - including a bare member resolved to its single defining file and a multi-candidate member
  surfaced as marked-unresolved. This criterion OWNS the re-projection and its resolution honesty.
- [ ] a test proves the DEFINED CELLS: a subject with no membership under the target lens returns
  the documented empty-cell body; a wide re-grain truncates with the count caption; a shared member
  appears once with the flag. This criterion OWNS the matrix completeness; it does NOT own
  re-projection mechanics (criterion 1).
- [ ] a test proves the RATIONALE OVERLAY data path: the per-node rationale query returns the
  decisions/findings/lessons attached to a fixture node (content fields only, deterministically
  ordered), nodes without rationale return none, and the batch endpoint covers a set of visible
  nodes in one request. This criterion OWNS the overlay data.
- [ ] a test proves the SERVED-PAGE WIRING: the page carries the lens selector with subject-sticky
  re-request, the overlay toggle with badge-and-expand disclosure, and with the composition absent
  and overlay off the existing views render byte-identical (structural assertions, as the viz tests
  do). This criterion OWNS the client seam.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
