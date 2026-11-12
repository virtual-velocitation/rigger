# The code store

The graph answers two kinds of question. Some are about the code: what a name resolves to, who
calls a function, which tests prove it, which community it belongs to. Others are about what the
project has decided and learned: decisions, review findings, lessons, verdicts and run state.
The first kind can be derived from the tree. The second cannot.

This addendum describes how rigger holds the first kind. The design has four parts:

- **The code store.** It holds code-derived facts. Each snapshot is stamped with the commit it
  describes and replaced file by file. It has no log.
- **The resolver.** One rule table that resolves every name at read time.
- **The composed view.** It joins the code store with the record graph, which the event log still
  folds.
- **The interface.** Every reader, and the worktree overlay, reads through it.

It is written as the target state.

---

## 0. Summary

```
 BEFORE                                          AFTER
 ======                                          =====

 tree (live bytes, any moment)                   tree at commit C (git objects, never live bytes)
   |                                               |
   | extract per file                              | advance(C): extract changed files only
   v                                               v
 event log  <-- generation entry per file        .rigger/store/code/<seq>.snap
   |            (history git already holds)        stamp = (HEAD ref, C, tree id, fingerprint)
   | fold                                          facts per file, names as written, no edges
   v                                               |
 graph.db: code + records in one SQL store         | open + honesty check (stamp tree vs worktree)
   - names resolved across files AT FOLD:          v
     staged refs, tier upgrades, proofs            CodeView ---- resolver (one rule table) ----+
     table, test-module closure                                                                |
   - records no commit; shifts at each merge                                                   v
   - 83.5% of edge rows dead                      event log (records only) -> record graph -> Composed
   |                                                                                           |
   v                                                                                           v
 readers (no way to know which tree)              readers: every answer names its commit and
                                                  says which of the caller's files differ
```

The code store answers from a named commit. It holds each file's facts as written. Every cross-file
rule runs at read, so a file changing never rewrites another file's facts, and the order things
happen in can never change an answer. The event log keeps only what cannot be re-derived from
the tree.

---

## 1. The problem, measured

The figures below are measured on this repository at one commit:

- the operator graph and the read-only event log, through `python3 -I` with read-only SQLite;
- a fresh clone, using the installed `rigger` binary.

**1.1 Every code fact goes through the fold.**

- A cold `rigger graph build` folded 142,593 events in 5 min 23.5 s. About 96% of that time
  (~2.2 ms per event) was the fold.
- Parsing the whole tree directly took 11.44 s on one core: 459 `.rs` files at ~25 ms each.

The fold costs about 28 times as much as the extraction it consumes.

**1.2 The graph records no commit.**

- Editing a file without re-indexing appends 0 events. Every answer about that file is then
  stale, and nothing says so.
- A later build appended 3,807 events in 3.58 s.
- Reverting the edit appended 4,238 events in 5.51 s.

The graph is a function of when ingests happened, not of a commit. At every merge it shifts under
any reader, and a spawned agent working in its own worktree is answered from the operator
checkout's state. Each lookup hit re-extracts its file to compare generations, which is a
per-hit check for a property the store could carry once.

**1.3 Cross-file resolution happens at write time.** Four rules resolve a name against other
files while folding:

- tagged references;
- call tiers;
- proofs;
- test-module exclusion.

Because of this the fold has to:

- **stage** references whose target file has not folded yet (7,596 rows in a fresh build);
- **upgrade** tiers when a definition arrives after its reference;
- keep a 99,623-row proof table;
- recompute an out-of-line test-module closure.

The result is a pure function of the final log only because every rule has a matching
reconciliation pass. That is the bug class: each new rule needs its own convergence argument.

**1.4 History git already holds is kept twice.**

The operator graph is 338.7 MB plus a 342.6 MB write-ahead log. Of its 289,777 edge rows, 47,777
are live (83.5% dead). It also holds:

- 46,854 retired-node rows;
- 4,167 detached-attachment rows.

Community membership is in the worst state: 4,110 of 4,281 recorded memberships are detached,
because the members they name were retired, and only 171 are live.

A fresh build at the same commit is 56.8 MB, with 13,671 nodes and 39,147 edges. On top of that,
the symbols index (`.rigger/symbols/index.json`, 31.5 MB) holds the same per-file facts a second
time.

**1.5 Derived rows in the log.** Of 80,806 events, 7,399 (9.2% of rows, ~1.05 MB of payload)
are derived:

| Event type | Rows | Payload |
|---|---|---|
| GenerationIngested | 2,781 | 0.44 MB |
| CommunityAssigned | 4,281 | 0.57 MB |
| ConceptRealized | 290 | |
| ConceptDerived | 47 | |

About 73% of the generation entries are superseded: 841 identities are covered and 761
generations are held. The cost is not mostly bytes. It is the machinery that folds these rows,
supersedes them and replays them.

**1.6 What replacing it costs (measured):**

| Operation | Cost |
|---|---|
| Whole-tree parse | 11.44 s on one core (~25 ms per file) |
| Communities pass | 1.99 s (410 communities over 7,256 nodes) |
| Concepts pass | 0.28 s (102 concepts over 816 nodes) |
| `git diff` of a worktree against a commit; the untracked list; `ls-tree` of a commit | under 10 ms each, on 769 tracked files |

A model of query-time resolution over the extracted facts (interpreted Python, used to size the
design; it is not a merge figure):

| Scope | Mentions | Time |
|---|---|---|
| Building the name indexes | | 11 ms |
| Product view | 34,301 | 21 ms |
| With tests | 164,734 | 92 ms |

How the product-view calls resolve:

| Outcome | Calls |
|---|---|
| Same-file | 4,315 |
| Unique project candidate | 3,373 |
| Frontier (several candidates) | 3,886 |
| No candidate | 13,033 |

788 mentions have a frontier of 10 or more; the largest frontier is 25.

**1.7 Test code dominates the facts.** Test code is:

- 65% of definitions (11,867 of 18,202);
- 79% of references;
- 323 of the 459 `.rs` files.

Any design that holds test code fully has to keep product views the size they are today.

---

## 2. Invariants

1. **Stamped.** The code store describes exactly one tree, named by its stamp. Every answer carries
   that stamp.
2. **Never stale.** An answer either matches the tree it claims to describe or says it does not.
   - Before the overlay, an answer names the stamp commit, lists how many of the caller's files
     differ from it, and withholds hits in those files.
   - With the overlay, it merges those files and describes the caller's worktree.
3. **Pure.** A snapshot is a pure function of (tree, fingerprint), and the fingerprint names
   everything that computes it: the extractors and the grouping algorithm. Advancing from A to B
   gives the same bytes as advancing from empty to B.
4. **Local facts.** A file's facts depend only on that file's path and bytes. Nothing is resolved
   across files at write time.
5. **One resolver.** Every cross-file rule exists once, in `rigger-domain`, and runs at read.
6. **Records stay.** Facts that cannot be re-derived from the tree (decisions, findings, lessons,
   verdicts, run state, aliases) stay in the event log and in the record graph folded from it.
7. **Nothing omitted.** All production code and all test code is held. Test code is marked and
   filtered by the view, never dropped at extraction.
8. **Atomic publish.** A reader sees one whole snapshot, old or new, never a mix.
9. **No external systems.**
   - Git is the version control the project already uses. A directory that discovery finds no
     repository enclosing reads its tree from the directory; git refusing inside a repository is
     an error, never a directory store.
   - Every OS facility sits behind an interface with an implementation for each OS.

---

## 3. Topology

```
                         +-------------------------- writers --------------------------+
                         |                                                              |
  conductor (after a     |   rigger graph build /       rigger setup                   |
  landed merge)          |   any reader whose stamp     (from empty)                   |
          \              |   is behind HEAD               /                            |
           \             +------------\-------------------/-----------------------------+
            \                          \                 /
             +------------------> code::open_current(root) --- LOCK held while advancing
                                         |
             TreeSource (Git | Dir) <----+----> Extract (per-language extractors)
             ls-tree,                     |       Group (communities, concepts)
             cat-file --batch             v
                               .rigger/store/code/
                                 CURRENT-<fp> -> "000042.snap"  (one per fingerprint)
                                 000042.snap   (immutable, crc-checked)
                                 LOCK
                                         |
                                         | read whole, verify crc
                                         v
   caller worktree --- TreeSource::changed_against_worktree ---> honesty label / overlay
                                         |
                                         v
                                 CodeView (stamp, files, defs_named, mentions_named, groupings)
                                         |
                                 resolver + ViewFilter (tests: exclude | include | only)
                                         |
                                         v
   event log --fold--> record graph --> Composed : ContextGraph <-- dash, CLI, MCP, lookup,
   (records only)      (graph.db,                                   conductor grounding
                        records only)
```

---

## 4. Data model

### 4.1 A file's facts

A file's facts come from its path and bytes alone. Each file is in one *half*:

- code, from the per-language symbol extractors;
- design, from the design-document extractor;
- workflow, from the workflow-definition extractor.

```
FileFacts
  path        "crates/x/src/a.rs"          (repo-relative, '/'-separated)
  blob        git blob id, or FNV content hash when there is no git
  lang        "rust" | "python" | ...      (from the extractor registry)
  half        Code | Design | Workflow
  scope       Product | Test               (path rule, see section 8)
  partial     true when the parse held an error, so defs and mentions cover only as far as
              the parse reached
  defs        [Def]                        (in the extractor's order)
  mentions    [Mention]                    (in the extractor's order)
  modules     [ModuleDecl]
  doc         [DocConcept], [DocLink]      (design half; names and targets as written. A code
                                            file also holds the rationale concepts and links
                                            found in its source)
  workflow    [WfNode], [WfRelation]       (workflow half; names as written)

Def
  local       "Outer::inner"               (path within the file; parent chain joined by ::)
  leaf        "inner"
  kind        "fn" | "struct" | "field" | ...
  line, end_line
  parent      index of the enclosing Def, or none
  test        true when the language marks it test-only (a #[test] fn, a #[cfg(test)] module, ...)

Mention
  enclosing   index of the enclosing Def, or the file
  name        the name as written: the reference's identifier ("bar")
  leaf        the final identifier of name; equal to name for every extractor today
  kind        Call | Tagged(<ref kind>)    (Call is the untagged reference)
  line
  test        true when inside a test-only def

ModuleDecl
  name        "foo"
  def         the index in this file's defs of the module definition that declares it
  test        true for a test-only module
  candidates  ["crates/x/src/foo.rs", "crates/x/src/foo/mod.rs"]   (from this file's path and
                                                                   any explicit path attribute)
```

A module's candidate paths depend only on the declaring file, so they are computed at
extraction. Picking the first candidate that is a file in the tree is a cross-file question, so
it happens at read (section 6).

### 4.2 How a name-reference is stored

A mention stores the name as written, its kind, its line and the def that encloses it. It
stores no target, no tier and no placeholder. The resolver works those out at read time.

When a file is added or removed, no other file's facts change, even if that file holds the only
definition a thousand mentions resolve to.

### 4.3 Entity ids

| Thing | Id |
|---|---|
| A definition | `<path>::<local>`, the scheme the graph uses today, so record edges that name an entity keep pointing at it |
| Two definitions with the same id in one file | settled by `definition_precedence(kind, line)` |
| A file | the id scheme it has today |
| A design document | the id scheme it has today |
| A workflow node | the id scheme it has today |
| A call placeholder `<file>::%<leaf>` | made by the resolver for a mention that resolves to no single definition; never stored |

### 4.4 The stamp

```
Tip                                  (what a TreeSource names: the tree to describe)
  head        the ref the operator checkout has checked out ("rigger-run"), for display
  commit      the commit id the snapshot describes (none for a Dir source)
  tree        the root tree id (Dir source: FNV over sorted (path, content hash))

Stamp                                (a Tip plus what only advance knows)
  tip         the Tip the snapshot describes
  fingerprint the extractor's fingerprint folded with the grouper's generation and the
              settled generation: changes when any extractor's output, the groupings or the
              rows settled at advance can change (the symbol extractors' tag version, the facts
              generation, the grouping generation and the settled generation; golden tests pin
              them to digests of the facts, the groupings and the settled rows over fixtures, so
              an unbumped change fails)
  seq         the snapshot's sequence number
```

A source returns a `Tip`. Only `advance` builds a `Stamp`, from a `Tip`, the folded
fingerprint and the sequence number it assigns, so no stamp ever holds a placeholder.

### 4.5 Snapshot layout

A snapshot is one immutable file. All integers are little-endian with fixed widths. A string is
a (u32 offset, u32 length) pair into one pool, and each distinct string is stored once.

```
 +-------------------------------------------------------------------------------+
 | header   magic "RGCS"  | u16 format version | u16 section count               |
 |          u64 fingerprint (extractor, grouper and settled rows)                |
 |          stamp: head (str), commit (str), tree (str), u64 seq                 |
 |          u32 crc32 of the header                                              |
 +-------------------------------------------------------------------------------+
 | section table: [u16 kind | u64 offset | u64 length | u32 crc32] x count       |
 +-------------------------------------------------------------------------------+
 | STRINGS    the pool                                                           |
 | FILES      path, blob, lang, half, scope, flags (partial, test file), then    |
 |            [start, end) into each of                                          |
 |            DEFS, MENTIONS, MODULES, DOC, WORKFLOW        (rows sorted by path) |
 | DEFS       local, leaf, kind, line, end_line, parent, flags   (one run a file) |
 | MENTIONS   enclosing, name, leaf, kind, line, flags            (one run a file) |
 | MODULES    name, flags, candidates                                            |
 | DOC        concepts and links as written                                      |
 | WORKFLOW   nodes and relations as written                                     |
 | IDX_LEAF   leaf -> [def row]       (sorted by leaf; binary search)            |
 | IDX_NAME   leaf -> [mention row]   (sorted by leaf; binary search)            |
 | GROUPINGS  communities at the default grain, their labels; concepts, their    |
 |            labels and REALIZES members      (computed over the production view)|
 | PROOFS     target, caller, file, line       (proof order: by target; binary   |
 |            search)                                                            |
 | IDX_PROOF_FILE  [proof row]                 (by file, then proof order)       |
 +-------------------------------------------------------------------------------+
```

- A reader parses the file from a byte slice it owns. Reading the whole file into memory is
  the access path. A memory mapping is a different owner of the same slice, so the format needs
  no change for one.
- **Size.** The tree holds 18,202 definitions and 164,734 mentions.

  | Section | Estimate |
  |---|---|
  | mentions | ~4.0 MB |
  | definitions | ~0.6 MB |
  | string pool | ~0.6 MB |
  | indexes | ~0.7 MB |
  | **total** | **~6 MB** |

  Today the graph is 338.7 MB plus its write-ahead log, and the symbols index is 31.5 MB.

---

## 5. The write path

### 5.1 advance

```
advance(store, source: TreeSource, extract: Extract, group: Group, target: Tip) -> Stamp

  fp <- fingerprint(extract, group)
  take LOCK (exclusive: a writer blocks; a reader tries once and, if the lock is held, answers
       from CURRENT-<fp> with the label "advance in flight")
  S <- snapshot named by CURRENT-<fp>; or EMPTY if there is none, its crc fails, or its
       fingerprint differs from fp; a store file that exists but cannot be read is an error
  if S.stamp.tip.tree == target.tree:      reap; release LOCK; return S.stamp      (no-op)
  entries <- source.list(target.tree)                       [(path, blob)] of each in-scope file
  keep    <- S.files whose (path, blob) is in entries       (facts copied, never re-extracted)
  fresh   <- extract every other entry, in parallel         (std::thread::scope, one chunk per
                                                            available core; bytes from
                                                            source.read_blobs, never the worktree)
  view    <- FactsView(keep + fresh)                         (in memory; indexes built)
  G       <- group.group(&view)                              (section 5.4)
  bytes   <- encode(view, G), then decoded back, every crc and reference checked: a snapshot
             that fails to read is an error, and nothing is published or reaped
  write   <seq+1>.snap.tmp ; fsync ; rename to <seq+1>.snap ; sync_dir
  write   CURRENT-<fp>.tmp ; fsync ; rename to CURRENT-<fp>  ; sync_dir
  reap    keep the two CURRENT-* naming the highest seqs; remove every other CURRENT-*, then
          every *.snap neither names, and every *.tmp (a failed delete is left for the next
          advance)
  release LOCK
```

```
   S (stamp tree T1)                     target (tree T2)
   +------+------+------+------+         git ls-tree T2, compared with S
   | a.rs | b.rs | c.rs | d.md |  ---->  M b.rs   D c.rs   A e.rs
   +------+------+------+------+
       |      x      x      |             cat-file --batch b.rs@T2, e.rs@T2
       | copy            copy |                     |
       v                      v                     v  extract in parallel
   +------+------+------+------+------+
   | a.rs | b.rs'| d.md | e.rs |      |  -> indexes -> groupings -> <seq+1>.snap -> CURRENT-<fp>
   +------+------+------+------+------+
```

What is *not* done, and why:

- **No resolution.** The resolver runs at read, so `b.rs` changing never touches `a.rs`'s rows.
  This is what removes staging, tier upgrades and the proof table.
- **No worktree bytes.** The shared store only ever describes a commit, so uncommitted edits in
  the operator checkout cannot leak into what a spawn reads. The overlay shows uncommitted edits
  to the one caller who has them.
- **No history.** A fingerprint's previous snapshot is reaped once its pointer names the new one.
  Git holds every earlier tree, and advancing to any commit rebuilds its snapshot.
- **No more than two fingerprints.** Two binaries sharing one store (an MCP server still on the
  previous build beside the newly installed CLI, a lane's dev build beside the installed one)
  each keep their own pointer and snapshot, so neither re-extracts after the other advances. The
  reaper keeps the two most recently advanced fingerprints and removes every older one's pointer
  and snapshot. Recency is the order the pointers were written in, which is the order of the
  sequence numbers they name, since every pointer is written under LOCK with the next number.

### 5.2 What advances the stamp

Only `advance` writes a snapshot, and it is reached through one function:

```
code::open_current(root) -> CodeView
  tip <- source.head()                                   (operator checkout's HEAD commit)
  S   <- snapshot named by CURRENT-<fp>
  if S is absent, unreadable, of another fingerprint, or S.stamp.tip.commit != tip.commit:
      advance(..., target = tip)           (on a lock or write failure: answer from S, labelled)
  return the view of CURRENT-<fp>
```

| Caller | When | Effect |
|---|---|---|
| conductor | right after it lands a merge into the checked-out branch | advances to the new tip, so the cost falls on the merge, not the next reader |
| `rigger graph build` | on request | advances to HEAD; replaces today's fold-based build |
| `rigger setup` | on install | advances from empty to HEAD |
| any reader | when HEAD moved without the conductor (a hand commit, a pull) | advances before it answers |

`root` is always the project root that owns `.rigger/`, which is the operator checkout. A caller
inside a unit worktree resolves `root` through git's common directory to that main worktree.
`source.head()` reads `root`'s HEAD, never the caller's. A reader in a unit worktree therefore
never advances the shared store to its own branch. Its own commit reaches it only through the
honesty check (6.1), and later through the overlay (13).

All four go through the same function. Writers wait for the lock. A reader takes it only if it
is free. When another process holds it (an advance is in flight), or the store is read-only, the
reader answers from the current snapshot with a label. That way a reader never blocks on, or
fails because of, a writer.

### 5.3 The merge that advances the shared store

```
 unit worktree (branch u/42)        operator checkout (HEAD = rigger-run)
        |                                    |
        | gates pass                         |
        +------- conductor merges u/42 ----->| new commit C'
                                             |
                                 code::open_current(root)
                                             |
                  ls-tree C'^{tree} against the snapshot's (path, blob) pairs
                                  -> the merged unit's changed files
                                             |
                                    advance(target = C')
                                             |
                     CURRENT-<fp> -> <seq+1>.snap, stamp.tip.commit = C'
                                             |
       every later spawn, lookup and dash view answers "describes C'"
```

This replaces the conductor's per-merge ingest of the named files and its whole-project ingest.
The changed files come from git's listing of C', not from the unit's report, so a file the unit
touched but did not name still gets re-extracted.

### 5.4 Groupings at advance

Community detection takes ~2 s, too slow to run per query. It is a pure function of the tree,
so it runs once per advance and is stored in the snapshot:

- **Communities.** The existing algorithm runs over the resolved *production* view: CALLS,
  REFERENCES and CONTAINS edges, excluding AMBIGUOUS edges and module-declaration CONTAINS, as
  today. Numbering is deterministic: communities are ordered by their smallest member id, so
  `community/<res>/<n>` is itself a pure function of the tree. Labels come from the existing
  labelling rule.
- **Concepts.** The existing derivation runs over the tree's intent layer: SPECIFIES,
  CONSTRAINS, GOVERNS, explains and references edges with a design-doc, arch-decision,
  handbook-rule or rationale endpoint. Edges between tree documents are inputs. Edges from
  decisions are not.

  This is a departure, so it is spelled out:

  | | |
  |---|---|
  | **Before** | 122 of the 3,662 intent edges (3.3%) came from decisions (decision GOVERNS design-doc: 95; decision GOVERNS handbook-rule: 27), so a new decision could reshape the clusters. |
  | **After** | Concepts cluster the tree alone. A decision is shown under the concept of the document it governs, attached at read by the composed view. |
  | **Why** | Groupings stored in a stamped snapshot must be a pure function of the tree. A grouping that read records would go stale at every recorded decision, and would need a record-driven recompute with its own lock and its own staleness. |
  | **Cost** | A concept that existed only because a decision bridged two documents splits into the documents' own concepts. Its decisions then show under each of them. |

- **Generation.** The grouping algorithm carries a generation (`Group::generation`), folded into
  the stamp's fingerprint, so a snapshot grouped by another algorithm is never trusted and is
  computed anew. A golden test pins it beside a digest of the groupings over a fixture, so an
  unbumped change fails.
- **Settled rows.** What a read would otherwise compute over the whole tree is settled once
  per advance beside the groupings, by the resolver itself (section 6.2), so a read takes rows:
  - **test-ness per file:** each FILES row's test flag, its Test scope or the test-module
    closure. Without it every reader read the facts of each file declaring a module and grew
    the closure before it resolved anything.
  - **proofs:** every test call that proves a product definition (`PROOFS`: target, calling test
    entity, file, line), in proof order, so the proofs of one definition are a binary search,
    and indexed by file, so the calls one test entity makes are the rows of its file. A whole
    read reads them instead of walking every test mention; a neighbourhood reads a
    definition's proofs and a test's attachment from them instead of resolving each mention of
    the leaf.
  - **definition leaves:** the keys of `IDX_LEAF`, which a lookup's name index reads.

  Their generation (`SETTLED_GENERATION`) is folded into the fingerprint like the grouping
  generation, and a golden test pins it beside a digest of the rows over a fixture. A view
  that holds no settled rows (an in-memory view) has the resolver compute them, and the two
  are tested to answer alike.
- A grain other than the default is computed on demand at read (~2 s) and is not stored.
- `rigger graph communities` and `rigger graph concepts` become read-only reports over the
  current snapshot. Nothing writes groupings anywhere else.

Grouping before and after:

| | Before | After |
|---|---|---|
| How a grouping is held | CommunityAssigned and Concept* events, folded | a snapshot section |
| When it is computed | on a verb run | at every advance |
| When a member is retired | the membership is detached (4,110 of 4,281 today) | the grouping is recomputed |
| Cost | | ~2.3 s per merge |

### 5.5 Write cost

| Step | One changed file | From empty (769 files) |
|---|---|---|
| ls-tree + cat-file | < 10 ms | ~ 50 ms (ls-tree + one batch) |
| extract | ~ 25 ms | 11.4 s / cores ~ 1.5-3 s |
| build indexes | ~ 15 ms | ~ 15 ms |
| groupings | ~ 2.3 s | ~ 2.3 s |
| write + fsync + swap | ~ 20 ms | ~ 20 ms |
| **budget (Done-when)** | **<= 3 s** | **<= 8 s** (today: 5 min 23.5 s) |

---

## 6. The read path

### 6.1 Opening a view and the honesty check

```
open(caller worktree W, filter)
  view  <- code::open_current(root)                               (read CURRENT-<fp>, read snap,
                                                                   verify crc: ~ 10 ms)
  diff  <- source.changed_against_worktree(view.stamp.tip.tree, W)  (git diff --name-status + the
                                                                   untracked list: < 20 ms over a
                                                                   refreshed index)
  diff empty      -> answers describe W exactly; label "describes <commit>"
  diff non-empty  -> without the overlay: label "describes <commit>; N files in W differ;
                     K withheld"; N counts only files in scope (`in_scope`, 7.1), so rigger's
                     own state never differs; every node in a differing file, and every edge
                     with such an endpoint, is withheld, and K counts the withheld hits
                     with the overlay (section 13): the differing files are merged in; label
                     "describes W (<commit> + N files)"
  diff fails (the stamp commit is not in W's object store, or W's index cannot be copied)
                  -> label "describes <commit>; W cannot be compared; K withheld"; it fails
                     closed: every node of the code half, and every edge with such an
                     endpoint, is withheld, and K counts the withheld hits; the
                     record half still answers
```

A node is in the file its facts came from, by one rule both arms read: a design or workflow
node's document (its `doc`; `stage:implement` is in `.rigger/workflow.yml`), any other code-half
node the file its id names, and a community or concept every file its attributes summarise (its
members', which give its label and hash, and its attached tests', which give its test count). A
differing file withholds the nodes in it; a worktree that cannot be compared puts every file in
doubt. Only code-half nodes are withheld: a record is never stale, so a record naming a differing
file is answered. An edge end is in the file its id names, as any other code-half node is, so an
end whose id names no file (a doc link to `glossary`) is in none and no comparison withholds it;
the edges joining it to a withheld node are withheld with that node.

K is the number of hits of this answer that were withheld, counted in the read that answered and
in one unit, nodes: the nodes it would have answered or reported hidden (`--around`, `--show`,
`--lookup`, the MCP graph tool, the dash), and for `rigger ground` the entities its withheld hits
name, keyed as its ranked page keys its rows, so a definition and its calls count once.

A walk (`--around`, `--show`, the dash's seeded neighbourhood and its card's one hop) counts the
withheld nodes it reaches and never steps from one; a withheld seed still reaches its members by
the member rule, which reads no edge. The dash's whole read keeps the part it withheld (the
withheld nodes and the edges dropped for a withheld end) beside the graph it answers, so each view
counts its own answer: a walk as above, a fold (a cluster drill, a subject's re-projection, the
rationale batch) the withheld nodes it would have held had nothing been withheld, and the
overview, a fold of the whole read, the read's own count. The directed-call view and the run
snapshot each read their own answer and count it as they read.

W is the caller's worktree: the git top-level of the cwd, else, outside any repository, the cwd;
a git failure inside a repository is an error. Every reader names it by that one rule (graph,
ground, lookup, MCP, dash), so a read from a subdirectory counts the whole worktree's differing
files and its label names the top-level.

The check never writes W's index. Every git the source starts itself passes
`--no-optional-locks` (discovery's `rev-parse`, which finds W's git dir, reads no index), and
since git still rewrites an index whose entries only a stat change has dirtied, the diff reads a
private copy of W's index instead: the index in W's own git dir (a linked worktree's included),
copied into a directory of its own in the OS temp directory with the index's mtime kept, so git
judges a racily clean entry as it would over the index itself. The directory goes when the read
ends; one a hard stop leaves is the OS temp cleaner's. A W with no index reads as git reads it:
every tracked file deleted. Until W's own git refreshes its index, each check re-hashes every
file the index cannot vouch for by stat: one whose stat alone changed, or one racily clean, as a
fresh clone's files are. A worktree rigger checks out for a unit is refreshed where it is
created: git judges racy cleanness to the second, and a checkout writes its files and its index
in the same second, so the create waits for the clock to pass that second (at most a second,
once per worktree) and then refreshes the index through git, and no read of it re-hashes a file.
The operator's own checkout is refreshed by their own git, which rewrites the index on any
status or commit; rigger never writes it.

A head with no commit (an unborn branch, which git answers by its quiet no that `HEAD` names no
object) reads as the empty tree, so every file in W differs until W commits. A head git fails to
read any other way, a commit it cannot read included, is an error, never the empty tree. Without git, the directory source counts a file in W it cannot read as
differing, since its content is unknown; its facts hold no such file. With git, the check
follows git's own view of W, as every git tool does: a file marked assume-unchanged or
skip-worktree reads as unchanged whatever W holds.

This check replaces the per-hit generation comparison. It runs once per view, not once per hit,
and the label travels with every answer:

- the CLI prints it;
- the MCP tools return it as a field;
- the dash sends it beside every graph reply (`X-Rigger-Graph-Label`, from the same open as the
  body) and shows it in the explorer's header; a graph it cannot read (none built, a store that
  cannot be opened) is a stated error (`X-Rigger-Graph-Error`) shown there, never an empty graph;
  the polled run snapshot (`/api/state`, and the static export built from it) carries the same
  label or reason as `graph.label` and shows it over the decision history.

The symbols grounder carries the label too (`Grounder::ground_labelled`, one open of the store for the rows and the label; the CLI prints it on stderr, beside
the rows). It applies the same withholding rule (`Withheld`, shared with the composed view). A store
that cannot be opened is read as a worktree that cannot be compared: every file is withheld, the
blast radius falls back to the text search, and the label names the failure; an unopenable store
never answers as an empty "no hit".

Freshness has this one mechanism, the label; no hit carries its own. A reader that cannot tell
whether a code hit is stale does not serve it, and the combined lookup's text half still answers
for the code.

`rigger graph --show` has no drift heal and no stale note: it bounds a shown body at the node's
own line. A differing file's nodes are withheld, and every code-half node is when W cannot be
compared, so a code node it shows is one the snapshot describes exactly, and the log holds no
code record a heal could have been reached through.

### 6.2 The resolver

One rule table in `rigger-domain/src/resolve.rs`. It moves today's rules unchanged; only the
time they run changes. These kinds are never call targets:

- **non-target kinds:** `field`, `property`, `declaration` (`NON_TARGET_KINDS`).

Candidates are definitions whose leaf equals the mention's leaf, found through `IDX_LEAF`. Where
the table says "candidates" it means call-target candidates: every kind except the non-target
kinds.

Which definitions a mention may resolve to depends on whether the mention is test code (section
8.1):

- **A product mention** resolves against product candidates only, under every filter. Test
  definitions sit in the same index, but they never change a product tier, a product frontier or
  a placeholder's INFERRED/AMBIGUOUS choice. Product answers are therefore the same whether or
  not tests are held.
- **A test mention** produces a proof under every filter. Under `include` and `only` it also
  produces the edges of the call and reference rules, resolved against all candidates, product
  and test. That gives test-to-test and test-to-product CALLS and REFERENCES edges, all marked
  `test`. Under `exclude` it produces no edge.

| Rule | Input | Resolution | Output (today's shapes) |
|---|---|---|---|
| call / untagged reference | Mention `Call` in file F | exactly one same-file candidate | EXTRACTED edge to that definition |
| | | otherwise | edge to the placeholder `F::%<leaf>`: INFERRED if any candidate exists anywhere, AMBIGUOUS if none |
| down-hop | a placeholder | one candidate | a hop to it |
| | | several candidates | a frontier |
| up-hop (callers of D) | definition D | direct resolutions to D, plus placeholder edges whose leaf is D's leaf | callers |
| tagged reference | Mention `Tagged(k)` | candidates restricted to `reference_candidate_kinds(k)`: exactly one same-file | EXTRACTED |
| | | else exactly one in the project | INFERRED |
| | | else | no edge; counted as unresolved |
| module | ModuleDecl in F | the first of its candidates that is a file in the view | CONTAINS edge to that file |
| proof | Mention `Call` that is test code: its `test` mark, a Test-scope file, or a file in the out-of-line test-module closure | exactly one same-file product candidate; else exactly one product candidate in the project | +1 to that definition's `proven_by`, and `file:line` added to its `proof_evidence` |
| duplicate id | two definitions with one id | `definition_precedence(kind, line)` | one node |
| kind precedence | several kinds for one id | `CODE_KIND_PRECEDENCE` | one kind |
| test-ness | an entity | section 8.1 | `test` attribute; drives the filter |
| design link | DocLink as written | the existing design-target rule, moved | SPECIFIES / CONSTRAINS / GOVERNS / explains / references |
| workflow relation | WfRelation as written | the existing workflow rule, moved | RUNS / NEEDS / REVIEWS |

Which facts live where:

| Fact | Before | After |
|---|---|---|
| a tier | stored at fold, upgraded when a later definition folds | a function of the view: no upgrade pass, no staging |
| a placeholder | a stored node | made per answer, the same id each time |
| proofs | a 99,623-row table | resolved from test call mentions at advance and stored as `PROOFS` rows (section 5.4) |
| the community unique-leaf rule | its own copy in `community.rs` (`Coupling::from_graph`) | the resolver's call rule; the copy is removed |

The `CodeGraph` adapter (`rigger-domain/src/code_graph.rs`) applies the table to a `CodeView` and
returns `Node`s and `Edge`s in exactly the shapes readers get today, placeholders and attributes
included. Code edges are always live: `valid_from = 0`, `valid_to = None`, `source = 0`. A code
edge has no log position, and readers only ever skip closed edges. The history of code is git's.

### 6.3 Resolving a neighbourhood

```
--around D (depth 1)

  out-edges of D   = resolve(D's mentions)            contiguous rows of D's file, filtered by
                                                      enclosing def = D (or its descendants)
                   + CONTAINS children of D           defs whose parent = D
  in-edges of D    = for m in IDX_NAME[D.leaf]: resolve(m) lands on D or on a placeholder of
                     D.leaf whose frontier holds D
  proofs of D      = PROOFS rows whose target is D                (binary search)
  community of D   = GROUPINGS lookup
  tests of a cluster = the callers of the PROOFS rows of its members, each kept when its
                     plurality cluster is this one: the targets a test entity proves are the
                     PROOFS rows of its file (IDX_PROOF_FILE) it calls from
```

A neighbourhood's test files are each FILES row's test flag, so opening one reads no
declaration and grows no closure (section 5.4).

The work is proportional to the number of mentions of one leaf. The largest frontier measured is
25 candidates.

A lookup term resolves the same way. The distinct leaves the definitions carry are its name
index; each name a tier reaches (exact, prefix, fuzzy, or the nearest names of a miss) reads
only the definitions of that leaf, and a name is a match only when one of those is shown. The
lookup never resolves the whole graph, and the hits it withholds are only those of the names it
reached.

### 6.4 Read cost

| Query | Work | Budget (through the binary) |
|---|---|---|
| open + honesty check | read ~6 MB, crc, two git calls | inside the budgets below |
| `--around <entity>` | the mentions of one leaf | <= 50 ms |
| whole default view (dash) | 34,301 product mentions + test call mentions for proofs | <= 150 ms |
| whole view with tests | 164,734 mentions | <= 300 ms |
| non-default community grain | groupings on demand | ~ 2 s |

The interpreted model resolves the product view in 21 ms and everything in 92 ms. Compiled code
does it in less, so no cache is kept: each view open resolves what its query needs.

The budgets assume the stamp is current. If HEAD moved without the conductor, the first reader
afterwards pays one advance first (<= 3 s for a small change, section 5.5).

---

## 7. Interfaces

### 7.1 The code-side interfaces (`rigger-domain/src/code_view.rs`)

```rust
pub trait CodeView: Send + Sync {
    fn stamp(&self) -> &Stamp;
    fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_>;   // path, half, scope
    fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>>;
    fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_>;
    fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_>;
    fn groupings(&self) -> &Groupings;
}

pub trait TreeSource {
    fn head(&self) -> Result<Tip, String>;                         // HEAD commit + tree
    fn list(&self, tree: &str) -> Result<Vec<Entry>, String>;      // path, blob
    fn changed_against_worktree(&self, tree: &str, worktree: &Path)
        -> Result<WorktreeDiff, String>;                           // changed + untracked paths
    fn read_blobs(&self, blobs: &[String], sink: &mut dyn FnMut(&str, &[u8])) -> Result<(), String>;
}

pub trait Extract: Send + Sync {
    fn fingerprint(&self) -> u64;
    fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts;   // path and blob set from the entry
}
pub trait Group {
    fn generation(&self) -> u64;                                   // folded into the fingerprint
    fn group(&self, view: &dyn CodeView) -> Groupings;
}

pub enum Tests { Exclude, Include, Only }
pub struct ViewFilter { pub tests: Tests }                         // default: Exclude
```

| Interface | Implementations |
|---|---|
| `CodeView` | `Snapshot` (rigger-store); `FactsView` (in memory: used by advance, the overlay and tests); `Overlay` (section 13) |
| `TreeSource` | `GitTreeSource` (rigger-worktree-git, every git it starts itself with `--no-optional-locks`: `ls-tree`, `diff --name-status` over a copy of the worktree's index (6.1), `ls-files --others --exclude-standard`, `cat-file --batch` through the existing `BlobBatch`); `DirTreeSource` (rigger-store: walks the directory with the one scoped walk, `rigger_store::walk::walk_scoped`, which the grounder walks with too, and hashes each file with FNV, for a project with no git, so `rigger graph build` outside a repository keeps working; it holds the last head's listing only, so a long-lived reader holds one listing) |
| `Extract` | `FactsExtractor` (rigger-grounder `facts.rs`, over the existing symbol, design and workflow extractors) |
| `Group` | `ProductionGroupings` (rigger-domain, over the existing community and concept algorithms) |

The store crate depends only on `rigger-domain`, std and the `ignore` gitignore engine.
Extraction and grouping are passed into it, so it holds no language knowledge. A file's scope
(Product or Test by path) is set by `Extract`, next to the per-language rules it comes from.

Which files a tree's facts cover is one rule, `rigger_domain::code_view::in_scope`: a path with
no hidden component, or one of `ALWAYS_IN_SCOPE` (the workflow file `.rigger/workflow.yml`).
Advance reads every source's listing through it, and the one scoped walk that `DirTreeSource`
and the grounder walk with applies it as it walks, entering a hidden directory only on the way
to an `ALWAYS_IN_SCOPE` path. Which files are the project's beyond that rule each source answers
by its own mechanism: `GitTreeSource` lists what git tracks, and the walk, with no index to read,
honours the committed `.gitignore`, which can leave out the workflow file as it can any other. A
name that is not UTF-8 is outside the scope, since the facts hold paths as text.

### 7.2 Splitting `Projection`

`Projection` today mixes four things:

- folding records;
- folding code generations;
- reporting generations;
- answering queries.

It splits by role:

```
                 Projection (today)
   apply, apply_batch, rebuild_owed,              subgraph, calls, resolve,
   apply_generation, current_generation,          locate, nodes_of_kind, whole
   held_identities
        |                                                   |
        v                                                   v
   RecordFold                                          ContextGraph
   apply, apply_batch, rebuild_owed                    subgraph, calls, resolve, locate,
   (graph.db, records only)                            nodes_of_kind, whole, stamp_label
        |                                                   ^
        |                                                   |
   apply_generation, current_generation,           Composed { records: &dyn Neighbors,
   held_identities: REMOVED (the stamp and                    code: CodeGraph<&dyn CodeView>,
   advance replace them)                                      filter: ViewFilter }
```

`Composed` implements `ContextGraph` with:

- **One traversal.** Breadth-first search over a `Neighbors` interface (`node(id)`, `out(id)`,
  `incoming(id)`), which both halves implement. The search is written once. The same interface
  answers the lookups a reader starts from: a file's members, the nodes a leaf binds, the nodes
  of a kind, a file's unresolved references, the recorded aliases and the whole half.
- **One identity.** The same id in both halves is one node. The tree half's kind wins, and the
  record half's attributes are merged in. A record endpoint that is in neither half (an ABOUT
  edge to a file the tree no longer holds) is a bare `artifact` node, as it is today.
- **Aliases at read.** Recorded aliases (AliasDefined) are applied when an id is resolved.
- **Groupings joined at read.** IN_COMMUNITY and REALIZES come from the snapshot's GROUPINGS. A
  decision attaches to the concept of the document it governs.
- **The label.** `stamp_label()` returns the honesty label from section 6.1.
- **Opened per read.** A reader holds an `OpenGraph`, whose `open(filter)` composes the record
  graph with the code view of the head as it stands then, so a reader that lives across merges
  (the conductor's grounding, the MCP server, the dash) follows the store as merges advance it.
  `open_unowed(fold, read, filter)` refuses with REBUILD_OWED first when the record fold owes
  its rebuild.

### 7.3 How today's readers move

| Reader | Today | Target |
|---|---|---|
| conductor grounding, reviewer and author context (13 `dyn Projection` sites) | `subgraph`, `calls`, `locate` on graph.db | the same calls on `&dyn ContextGraph`, opened per read through `Deps.context` (an `OpenGraph`); `Deps.graph` is the `RecordFold` |
| conductor ingest after merge | `ingest_files_into_graph`, `ingest_project_into_graph`, `apply_generation`, `current_generation` (24 sites) | `code::open_current(root)` |
| `rigger graph --around / --show / --lookup` (`src/cli/graph.rs`) | `dyn Projection` (5 sites), generation reads (9 sites) | `&dyn ContextGraph`; the label printed first |
| `rigger graph build / communities / concepts` | fold, then write groupings as events | `open_current`; read-only reports |
| MCP `rigger_graph`, `rigger_ground` (`mcpserver.rs`, 7 sites) | `dyn Projection` | `&dyn ContextGraph` + a `tests` argument + the label field |
| combined lookup (`lookup/graph.rs`, `names.rs`, `prose.rs`, `text.rs`) | `Result<&dyn Projection, String>` and the per-hit freshness check | `Result<&dyn ContextGraph, String>`; freshness removed; withheld hits counted |
| dash `/api/graph`, explorer, lenses (`dash.rs`) | `whole()` on graph.db | `whole()` on `Composed` with `tests=exclude\|include\|only`; the label of the same open in the header, a graph it cannot read a stated error |
| symbols grounding (`SymbolGrounder`, registry, `staleness`) | `.rigger/symbols/index.json` | `CodeView::defs_named` / `file` |
| console map (`console/map.rs`) | CommunityAssigned folded | GROUPINGS through `ContextGraph` |
| record fold (`sqlite.rs`) | `impl Projection` | `impl RecordFold + Neighbors` (records only) |

Readers that call query methods keep their call shapes, so the move is mostly a change of type.
Readers of generations disappear.

---

## 8. Test code

### 8.1 How it is held

Test code is extracted fully: every definition and mention, in both test files and test-only
items in product files. An entity counts as a test entity when any of these holds:

1. its definition carries the language's test mark (`Def.test`); or
2. its file's scope is Test by the path rule:
   - directories named `tests`, `benches`, `examples` or `__tests__`;
   - Go `_test.go`;
   - Python `test_*.py` and `*_test.py`;
   - JS/TS `.test.` and `.spec.`;
   - C# directories ending in `Test` or `Tests`.

   The rule stays, unchanged, with the per-language extractor registry. It is applied once, at
   extraction, and stored as `FileFacts.scope`; or
3. its file is in the out-of-line test-module closure: files reached only through test-only
   module declarations. This is computed per view open from the MODULES section, using the same
   `module_target` function the module rule uses, so the two cannot disagree.

### 8.2 How it is filtered

```
                         Tests::Exclude (default)       Include               Only
 communities, concepts   product members only (always); product members;      test entities, grouped
                         each community shows           tests shown as a      by the community they
                         "N tests" (its plurality-      "tests" group per     prove
                         attached test count)           community
 lenses                  product; "N tests hidden"      product + tests       tests
 concept evidence        product; "N tests hidden"      both                  tests
 lookups (CLI, MCP)      product hits; test hits as     all hits, marked      test hits
                         a separate count
 explorer code view      product; "Show tests" toggle   product + tests       tests
 proofs (proven_by,      always computed from test      same                  same
 proof_evidence)         mentions
```

- **Groupings never cluster test code.** Test code is 65% of definitions and 79% of references,
  and it would swamp every community. When tests are included, a test entity is shown attached
  to the community that holds the plurality of the product entities it proves. It is drawn as
  that community's "tests" group, with its count, and never as a member.
- **The proof count per product entity stays,** with the same attributes as today. Proofs read
  test mentions whatever the filter is, so excluding tests never zeroes a proof count.
- **The filter is a parameter of the view:**
  - the dash takes `tests=exclude|include|only` on `/api/graph`, and the explorer's "Show tests"
    toggle sets it;
  - the MCP tools and the CLI take `tests` / `--tests`.

---

## 9. Records and composition

The record graph is graph.db, folded from the event log. It holds only records:

- **Nodes:** decision, finding, lesson, artifact.
- **Edges:**
  - ABOUT (finding or lesson to an artifact, file, design-doc or handbook-rule);
  - GOVERNS whose source is a decision;
  - SUPERSEDES.

Live today: ABOUT 3,536, GOVERNS from decisions 1,846 and SUPERSEDES 85. No live record edge
names a community or concept id (measured: 0).

**Record targets are stored as written.** The record fold resolves a target through the
recorded aliases only, and never consults a code, file or document node. It creates the target
as a bare `artifact` node when no node with that id exists yet. Many record edges land on ids
that the tree also holds. Measured live:

| Relation | Target kind | Edges |
|---|---|---|
| ABOUT | file | 1,413 |
| GOVERNS | file | 960 |
| ABOUT / GOVERNS | design-doc | 209 |
| ABOUT / GOVERNS | handbook-rule | 57 |
| GOVERNS | code-entity | 10 |

In the records-only graph.db those targets are `artifact` nodes. In the composed view the same
id is one node with the tree's kind, so readers see exactly what they see today.

The fifteen graph.db tables are disposed of as follows:

| Table | Measured today | Disposition |
|---|---|---|
| nodes | 19,369 live | record kinds only |
| edges | 289,777 rows, 47,777 live | record relations only |
| node_assertions | per-file identities + record rows | record rows only (decision 16,001; finding 9,264; artifact 939; lesson 321); community and concept rows go |
| edge_assertions | 10,632, every row a per-file identity | removed |
| refs | 7,596 staged (fresh build) | removed: resolution is at read |
| proofs | 99,623 (fresh build) | removed: proofs are counted at read |
| proofs_owed | 0 | removed |
| generations | one row per ingested identity | removed: the stamp replaces it |
| retired_nodes | 46,854 (artifact 46,153; code-entity 559; rationale 141; gate 1) | record nodes only |
| detached_attachments | 4,167, all naming nodes no longer held | removed: groupings are recomputed per stamp |
| relabel_owed | 0 | removed: labels are computed at advance |
| aliases | 0 | stays (records) |
| applied | 80,228 | stays: the fold's applied set |
| caught_up | the fold cursor | stays |
| passed_over | 642 resolved-to-nothing (a generation entry whose re-extraction found nothing) + ~150 payload-rejected (on DecisionMade, LessonLearned, ReviewFinding) | payload-rejected rows stay; resolved-to-nothing rows go with the ledger |

The views `live_node_assertions` and `live_edge_assertions` follow their tables. The transient
rebuild tables (`rebuild_cursor`, `rebuild_run_closure`) stay with the record fold.

```
 event log ----fold----> graph.db (records)         .rigger/store/code/<seq>.snap
                           decision, finding,          files, defs, mentions,
                           lesson, artifact            modules, doc, workflow,
                           ABOUT, GOVERNS, SUPERSEDES  groupings
                                    \                        /
                                     \                      /  resolver + filter
                                      v                    v
                                 Composed: one id = one node; tree kind wins;
                                 record attrs merged; aliases applied;
                                 missing record endpoint -> bare artifact
```

---

## 10. What leaves the event log

These types join the refused derived list. The log's appender rejects them, as it already
rejects the four extraction types (CodeEntityExtracted, EdgeInferred, DocConceptExtracted,
DocLinkExtracted):

| Type | Rows today | Replaced by |
|---|---|---|
| GenerationIngested | 2,781 | the stamp |
| CommunityAssigned | 4,281 | GROUPINGS |
| ConceptDerived | 47 | GROUPINGS |
| ConceptRealized | 290 | GROUPINGS |

This machinery goes with them:

- the generation ledger reader (`LoggedGenerations`);
- the ledger's batch helpers (`entry_of_batch`, `batch_is_current`);
- the index-lag advisory: `rigger validate` prints the stamp line and the count of files the
  operator checkout differs by instead;
- the `excluded` flag on ledger entries;
- the ledger causes in `passed_over`;
- the symbols index with its staleness advisory and the `rigger reindex` verb.

What stays:

- every record type (DecisionMade, ReviewFinding, LessonLearned, AliasDefined and the rest);
- the episodic types (FileTouched, BlastRadiusComputed);
- run state.

---

## 11. Migration

Migration runs inside `rigger setup` (and the next owed rebuild), once per project. Every step
is idempotent.

```
 step 1  advance code store from EMPTY to HEAD            .rigger/store/code/ appears
            |
 step 2  PROJECTION_VERSION 6 -> 7: the existing owed-rebuild path refolds graph.db
         from record events only (derived types skipped by type) into graph.db.next
            |
 step 3  compare: every live decision, finding and lesson node (with its attributes),
         every live and retired record edge (from, to, rel, valid_from, valid_to, tier),
         every record assertion and every alias of the old graph.db is in graph.db.next
         (endpoints compared by id: a record target the tree half held is now a bare
         artifact, which is expected)                       missing -> refuse, keep old file,
            |                                                          print what is missing
 step 4  check: no record names a community/ or concept/ id   any -> refuse, print them
            |                                                 (measured today: 0)
 step 5  swap graph.db.next -> graph.db (existing rebuild swap)
            |
 step 6  rigger reset --derived: one transaction deletes every derived row from the
         event log (GenerationIngested, CommunityAssigned, ConceptDerived, ConceptRealized,
         and any remaining extraction rows), then reclaims space
```

- **No loss.** Steps 3 and 4 prove that every non-derivable fact survives before anything is
  deleted.
- **Scope.** Step 4 runs on every refold to rule 7, whatever rule the old file records; step 3
  runs from the older rules that fold records as rule 7 does (5 and 6, since rule 5 last changed
  the record fold), because a file an older rule folded holds record facts the records no longer
  fold to, and a rebuild at rule 7 refolds the log it is handed, a removed or other log included.
- **Rebuild lock.** Step 6 reuses the existing verb, its rebuild lock and its refusal while a
  live writer holds the log. Its rewrite-to-ledger-entry step is removed, because nothing derived
  is kept.
- **Order.** Step 6 runs only after step 5 succeeds. `rigger setup` runs it itself, under the
  rebuild lock it already holds, once the swap has succeeded; while the run is live it deletes
  nothing, says so, and leaves the rows for `rigger reset --derived`.

---

## 12. Crash safety

| Crash point | State left | Recovery |
|---|---|---|
| during extraction or before the snapshot rename | the pointer names the old snapshot; a `*.tmp` orphan | readers see the old stamp; the next advance reaps the orphan and redoes the work |
| after the snapshot rename, before the pointer rename | the pointer names the old snapshot; a complete unreferenced `<seq+1>.snap` | the same; the reaper removes the unreferenced snapshot |
| after the pointer rename, before or during the reap | the pointer names the new snapshot; old files remain, or an older fingerprint's snapshot whose pointer is already gone | the next advance reaps them |
| a torn or corrupt snapshot | a crc check fails on open | treated as absent: `open_current` advances from empty (<= 8 s) |
| an older OS loses the directory entry of a rename | the pointer names the old snapshot | the old stamp is a true statement about an older commit; the next open sees HEAD ahead and advances |
| a reader races the reaper | the snapshot named by a just-read pointer is gone | the reader re-reads the pointer and retries, at most 3 times |
| two advancers | the second waits on LOCK | it re-reads its pointer under the lock: no-op if it is already at the target, otherwise it advances from the new stamp |
| a crash during migration | steps 1-5 write only new files until the swap; step 6 is one transaction | a rerun resumes at the first step that is not yet done |

---

## 13. The worktree overlay

The overlay lets a caller whose worktree differs from the stamp see its own tree. It is a
decorator over `CodeView`, and it reads through one more method of an existing interface:
`TreeSource::changed_against_worktree`.

```
Overlay { base: &dyn CodeView, changed: FactsView, masked: set of paths }

  open(base, W):
    diff    <- source.changed_against_worktree(base.stamp.tip.tree, W)
    masked  <- diff.modified + diff.deleted
    changed <- extract(read W/<p>) for p in diff.modified + diff.added + diff.untracked
               (memoised per (path, content hash) for the life of the process)
  defs_named(leaf)     = base.defs_named(leaf) not in masked  ++  changed.defs_named(leaf)
  mentions_named(leaf) = base.mentions_named(leaf) not in masked ++ changed.mentions_named(leaf)
  file(p)              = changed.file(p) if p changed, None if deleted, else base.file(p)
  stamp                = base.stamp + "N files from W"
  groupings            = base.groupings (labelled as describing the stamp)
```

The resolver runs over the merged view, so tiers, proofs and test-ness describe W exactly.
Groupings still describe the stamp: a new entity shows no community until the next advance, and
the label says so.

The overlay is about 300 lines plus tests. Its cost is extraction for each changed file (~25 ms,
in parallel) plus the git diff (< 20 ms over a refreshed index, 6.1). With it, the honesty label
in 6.1 becomes "describes W", and withholding is no longer used.

---

## 14. Fit with the owned store

The owned-store design (the owned-store addendum and specs 109-111) replaces SQLite with
rigger's own segment log (109), a resolved graph held in memory and loaded from a base snapshot
(110), and an owner lock for structural work (111).

**Reused from the code store:**

- **Format.** `rigger-store/src/format.rs` is written once and used by the code snapshot, the
  segment log and the record base snapshot. It holds:
  - the magic plus version header;
  - the crc32 check;
  - the section table;
  - the atomic publish (temp, fsync, rename, `sync_dir`);
  - the lock (`with_write_lock`, on std `File::lock`).

  109 and 110 add their modules (`segments`, `graph`) to the same crate.
- **Publish convention.** The pointer convention (a small file naming an immutable snapshot,
  replaced by an atomic rename) is the one a MANIFEST uses. The owned
  store keeps its manifest beside `store/code/` and follows the same convention.

**Changes to the owned-store work:**

- **109:**
  - the kind-1 ledger record goes;
  - the footer's per-file latest-generation map goes;
  - the manifest's ledger segment goes;
  - the latest-generation read goes;
  - the record kinds that remain are event (0) and progress (2).
- **110:**
  - the base snapshot holds the record graph only;
  - the rebuild source is the record journal alone;
  - the pending-proof table and code nodes and edges leave the resident graph;
  - its size goals shrink to the record graph (19,369 live nodes today, of which 12,426 are
    code entities that move out).
- **111:**
  - `advance` is a locked write that any process may make (the conductor, the CLI, a reader whose
    stamp is behind HEAD). It is not owner-only structural work, and it takes the code store's
    own LOCK, not the OWNER lock;
  - the owner may keep the current snapshot open and count its bytes.
- **Addendum:**
  - section 4's perception row moves to the code store;
  - invariant 3 reads "rebuildable from the journal and the tree", with no ledger;
  - section 5.7's ledger fold goes;
  - section 5.8's verify-on-mount rebuilds records from the journal and advances code from
    empty;
  - acceptance 10.1 compares the record graph plus a code snapshot advanced from empty.

**Made moot:** the generation ledger in all its forms. That covers perception as a ledger entry,
the memo that proves its own currency, the ledger's answer to the index-lag advisory, and the
conversion of the derived backlog into ledger entries.

**Landing order:**

1. the code store PR;
2. the overlay PR;
3. 109, amended;
4. 110, amended;
5. 111, amended.

The code store lands first for three reasons:

- it removes the ledger kind and the per-file generation map from 109, and the code half from
  110's graph;
- it ships the `format` module that 109 and 110 then extend rather than invent;
- it is reachable with SQLite still in place, because the record fold is untouched apart from
  dropping derived types.

---

## 15. Cross-platform and agnosticism

| OS facility | Interface | Unix | Windows | macOS |
|---|---|---|---|---|
| exclusive lock | `with_write_lock` (std `File::lock` / `File::try_lock`) | `flock` | `LockFileEx` | `flock` |
| atomic replace | `rename` (std) | `rename(2)` | `MoveFileExW` replace-existing (std) | `rename(2)` |
| make a rename durable | `sync_dir(dir)`; a filesystem that cannot sync a directory (InvalidInput, Unsupported) leaves the rename standing and is not an error, as in SQLite | open the directory + `fsync` | no-op: a directory handle cannot be flushed portably; a lost rename leaves the pointer at the old, still-true stamp (section 12) | open the directory + `fsync` |
| delete a file another process has open | reaper | unlink | std opens with share-delete, so the delete succeeds; a mapped file's delete fails and is retried at the next advance | unlink |
| read the tree | `TreeSource` | git or the directory | git or the directory | git or the directory |

Snapshots are immutable files named by sequence number, never replaced in place. On every OS,
a reader holding one open (or mapped) never blocks a writer.

- **Language-agnostic.** Extraction goes through the per-language extractor registry. The
  resolver reads kinds and names only (`reference_candidate_kinds`, `NON_TARGET_KINDS`,
  `CODE_KIND_PRECEDENCE`), which the registry already defines per language. The store crate
  holds no language knowledge.
- **Process-agnostic.** The store knows nothing of runs, units or gates. It describes a commit,
  and whoever moves HEAD calls `open_current`.
- **No external systems.** Git is read through the existing worktree crate, and no new
  dependency is added. Discovery (`rigger_worktree_git::discover::rev_parse`) alone decides that
  no repository encloses a directory, which then uses `DirTreeSource`; git refusing inside a
  repository is an error wherever a repository is resolved, never a directory store.

---

## 16. Acceptance

**Behaviour:**

- `advance_is_path_independent`: advancing A->B gives the same bytes as advancing empty->B.
- `advance_reextracts_only_changed_blobs`, using a counting `Extract` double.
- `advance_reads_git_objects_never_worktree_bytes`, using a worktree whose bytes differ from HEAD.
- `a_reader_in_a_unit_worktree_advances_from_the_operator_checkout_head`, using a unit worktree
  on a branch one commit ahead: the stamp stays on the operator checkout's HEAD, and the reader's
  label counts the differing file.
- `a_fingerprint_change_advances_from_empty`.
- `a_change_to_the_facts_ships_with_a_generation_bump`: the facts generation is pinned to a digest
  of the facts read over the golden fixtures.
- `a_corrupt_snapshot_is_treated_as_absent`.
- `a_reader_never_sees_a_partial_snapshot`, and
  `a_crash_between_renames_leaves_the_old_stamp_current`, both using a fault-injecting publish.
- `two_advancers_converge_on_one_snapshot`.
- `code_view_answers_the_golden_of_the_retired_fold`: a fixture tree with a row for every
  resolver rule, checked against a golden captured from today's fold.
- `a_file_change_never_rewrites_another_files_facts`.
- `product_tiers_ignore_test_candidates`: a `#[cfg(test)]` helper sharing a product leaf
  changes no product tier, frontier or placeholder choice.
- `migration_keeps_a_decision_governing_a_tree_file`.
- `the_honesty_label_names_the_stamp_and_counts_differing_files`, and
  `hits_in_differing_files_are_withheld_and_counted`.
- `proof_counts_survive_the_tests_exclude_filter`.
- `groupings_never_cluster_test_entities`.
- `tests_include_attaches_each_test_to_its_plurality_community`.
- `migration_refuses_when_a_record_would_be_lost`, and
  `migration_refuses_when_a_record_names_a_grouping`.
- `derived_types_are_refused_by_the_log`: GenerationIngested and the three grouping types.
- `overlay_merges_changed_files_and_describes_the_worktree`.

**Cost (wall clock through the installed binary on this repository; Done-when also asserts
operation counts through a counting view double):**

| Operation | Budget |
|---|---|
| `--around` | <= 50 ms |
| whole default view | <= 150 ms |
| advance after one changed file | <= 3 s |
| advance from empty | <= 8 s |

**One-time equivalence (run once on this repository before the fold is deleted):** the composed
view at a commit is compared with a fresh fold-based build at the same commit. Differences are
listed by class. Only these classes are expected:

- dead rows the fold kept;
- groupings recomputed and numbered deterministically;
- concepts no longer clustering decision edges;
- test entities present under `tests=include`.
