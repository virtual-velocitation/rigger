# 119 - A node is addressed by its path and shown by one renderer

**Goal:** a persona reaches a definition by the name its language gives it and reads what the graph
knows about it in one place. Today every code node hangs off its file only (`CONTAINS` file ->
entity, `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:2718-2727`), two `new` methods in
one file share the id `<file>::new` (`sqlite.rs:3291`), every resolver cuts an id at its first `::`,
so a nested id would answer `Type::method`, and `show` drops every attr but kind, line and degree
(`EntitySite`, `crates/rigger-domain/src/contextgraph.rs:396-409`), while its body bound comes from a
second tag-range reader beside `extract`. This spec gives every definition an id that follows its
language's path under a `CONTAINS` edge from its parent, resolves every reference and bare `show` by
the definition's leaf, and prints one closed attr list through one renderer on every surface.

## Design

**IDS FOLLOW THE LANGUAGE'S PATH, decided here.** `Def.path` (criterion 6's) is a definition's id path in
the file, and `Def.parent` (criterion 9's) its innermost enclosing definition's path of any kind,
absent at file level; `name` stays the leaf; every comparison of an id carrying a path against a `Def` is by `Def.path`, `locate_extent`'s match (ONE EXTENT) the id scheme's own reader. The node id is `<file>::<path>`: the parent's path,
`::`, the leaf, but `.` before a `field` or `property`; an impl contributes its self type as
`impl_block` spells it, each whitespace run one space and the ends trimmed (`normalize_ws`, `extract.rs:906`), differently spelled headers distinct paths, accepted (`<file>::Type::method`, `<file>::Foo<u8>::new`), or `<SelfType as Trait>` (`<file>::<Type as Trait>::method`), both halves spelled as `impl_block`'s `text` gives them, path qualifiers and generics kept and whitespace collapsed (`impl fmt::Display for T` giving `<T as fmt::Display>`); every path, an impl's and its members'
included, begins with the path of the definitions enclosing it: `mod m { struct T; impl T { fn
new() {} } }` gives `<file>::m::T`, `<file>::m::T::new` and the impl `<file>::m::impl T`. A member's path prefix is its parent's path, except that an impl parent contributes its self type (or `<T as Trait>`) in place of its header, so `parent` and the prefix differ only there and at a Go method whose receiver type the file does not define (parent the file, prefix the receiver). So:
`<file>::Enum::Variant`, `<file>::Struct.field`, `<file>::Struct.0`, `<file>::Enum::Variant.a`,
`<file>::Type::NAME`, `<file>::Trait::method`, `<file>::outer_fn::inner`, `<file>::m::item`; a Go method is `<receiver base type>::<name>`, the receiver base type being spec 116's base-identifier rule for a Go embedded field (pointer, package qualifier and type arguments stripped); a file-level impl is `<file>::<header>`. The parent is the
innermost definition whose byte range STRICTLY contains it (`enclosing_def`'s containment without the caller rule's declaration skip and function-like preference, over the name-filtered ranges), a Go method's the definition whose `Def.path` equals its receiver base type (a file-level type only), else the file; definitions of equal range (a
Go `const a, b = f(), g()`, `var a, b int`, a field list `x, y int`) are siblings under the next enclosing parent, and a definition two equal-range siblings both contain is their sibling there (the prefix computation's, criterion 6's; `Def.parent` at 9 exposes the chain 6 computes). A name-filtered
definition contributes nothing to a path: the enclosing-path prefix (criterion 6's) and `parent` read the filtered range vector `def_ranges` the
caller rule reads (spec 116), so its children take the next enclosing definition, else the file. A C# `file_scoped_namespace_declaration`, whose node holds only its name (the
vendored grammar's `node-types.json` gives it a `name` field and no body, the declarations after it
being its siblings under `compilation_unit`), is the parent of every declaration after it that no other definition strictly contains, and the path prefix of all of them (`N::Foo::M`, parent `N::Foo`), so `namespace N; class Foo {}` gives `<file>::N::Foo` as the block form does. A Go
`package_clause` names a package that spans a directory, not a nesting inside the file, so ids stay
file-scoped and the package is no path component, accepted. A leaf may hold `.` or `::` (spec 116's
dotted namespaces and string member names), so two definitions can spell one path (a TS
`namespace A.B` beside `class A { B }`, both `<file>::A.B`) and fold to one node by ONE PATH, ONE
EVENT, accepted beside the accepted landing cases. A named `function_expression`,
`generator_function` or `class` value whose name equals the leaf of the declarator, assignment, pair or class-field
definition enclosing it takes that definition's path and parent (`const f = function f() {}`, `const Foo =
class Foo {}`, `{ foo: function foo() {} }`, `class A { f = function f() {} }`), one node by ONE PATH, ONE EVENT, its kind agreeing under spec 116's value-kind rule.
`Def.enclosing_inline_module_path` (`symbols/model.rs:104-123`, set at `extract.rs:410-411`) stays a distinct fact: it is an out-of-line module's directory key, rustc's `#[path]` chain that skips enclosing fns, not the byte-containment chain `parent` records. `SymRef.enclosing` and
`EdgeInferred.caller` (criterion 8's) carry the caller's path in place of its name; an old
payload's caller folds to the same flat id its old definition event folds, and its batch
re-records under 107. Their writers and readers, from a literal search of production `.rs` files in `crates/*/src` and `src` (`#[cfg(test)]` excluded) for `.enclosing`, `.caller`, `enclosing:` and `caller:`, each given one disposition:

| Site (595cbc59) | Disposition |
|---|---|
| `extract`'s attribution (`symbols/extract.rs:364`, `enclosing_def`) | REPLACED: the enclosing definition's `Def.path` in place of its name |
| `extract`'s two top-level constructions (`extract.rs:287`, `:305`) and the `caller: None` writers (`symbols/events.rs:344`, `:455`, `:492`) | UNAFFECTED: a top-level reference keeps `None` |
| `extract_events`' copy into `EdgeInferred.caller` (`symbols/events.rs:262`) | KEPT: it copies the path |
| the reference fold arm's caller read (`sqlite.rs:2855-2856`) | REPLACED: it names the caller node `<file>::<path>` |
| the field docs (`symbols/model.rs:126-133`, `contextgraph.rs:945-953`), "the caller's name" | rewritten to the caller's path |

The payload carries both: `CodeEntityExtracted.path`, optional and absent
meaning the name, the entity arm building the id from it through `code_entity_id(file, path)`
(`sqlite.rs:2690`, `:3291`), which is this block's and lands with the first emitter of `path` (it
has no producer before criterion 6's, so it is criterion 6's), and `CodeEntityExtracted.parent`
(criterion 9's), optional; an impl member's path is not derivable from parent and leaf. `Def.path`
is always written (PAYLOADS), so a `Def` never lacks it. 
`Def` gains `end_line` (ONE EXTENT, criterion 5's), `path` (criterion 6's) and `parent` (criterion
9's). Its readers, from a literal search of production `.rs` files for `Def {`, `.defs` and `struct Def`, each given one disposition:

| Reader of `Def` (595cbc59) | Disposition |
|---|---|
| `unresolved_def` (`extract.rs:162-172`), spec 116's one constructor, through which `record_def` (`:194`) and the impl loop (`:299`) build every tagged, read-off and impl `Def` | REPLACED by `def_of` (criterion 5's) |
| `extract_events` (`events.rs:218-220`) | reads `path` (6), `parent` (9), the parent-before-child sort (16) and the same-path rule (10); still drops `is_test` |
| `out_of_line_test_module_files` (`events.rs:761`, `:777`) | reads none of the new fields, unchanged |
| `name_occurrence_map` (`grounder.rs:294`), `scored_hits` (`:491`), `blast_radius` (`:628`), `ground_ranked` (`:778`) | read none of the new fields |
| the index store (serde, `model.rs:50-51`) | persists `end_line` (criterion 5's) and `path`, always written, and `parent` |
| `extract::locate_extent` (criterion 5's) | reads `end_line`, and `path` from 6 |
| `definition_extents`, `DefExtent` (`extract.rs:1310-1377`) | retire at 5 |

**ONE PATH, ONE EVENT, decided here.** Definitions sharing a path in one file (overloads, cfg twins,
impls with one header, `mod x;` beside `fn x`) are one node: `extract_events` emits one event per
path (criterion 10's), so the fold never sees twins (a hand-built pair settles as spec 116's criterion 5, A SHARED NODE TAKES ONE KIND, has it). ONE total order picks the winner everywhere: among the
definitions surviving `extract_events`'s test filter (`symbols/events.rs:220`), the first by spec
116's `CODE_KIND_PRECEDENCE` (its criterion 5), then the lower line, then the order `extract`
returns them (the tags query's match order over one file's bytes, deterministic for them);
`extract_events`' dedup keeps exactly that definition, which gives name, line, kind and parent, only its parent edge being emitted; each
generation re-derives the winner, so removing a twin leaves the remaining one's. The dedup groups a file's definitions by `Def.path` alone, each group's winner by that total order; the emission order is A PARENT IS EMITTED BEFORE ITS CHILD's and a renamed path's retirement A RENAMED PATH RETIRES ITS OLD LEAF's, neither the dedup's.

**A PARENT IS EMITTED BEFORE ITS CHILD, decided here.** `extract_events` emits a file's definitions sorted by the length of each one's parent chain, each parent resolved to the definitions emitted at its path and the longest chain taken, then leaf, then path, in place of today's keys (`symbols/events.rs:221-226`). The sort is stable (criterion 16's, landing between 9 and 10): from criterion 10, one definition emitted per path, two distinct paths never tie and emission is a total order independent of `extract`'s, and on tree 16 same-path twins keep `extract`'s order. A parent's own event thereby precedes every child's `ensure_node`; a hand-built child-first log, and tree 9's extraction before 16 sorts it, transiently retires the parent's definition (a child's bare assertion of the parent replacing its prior one and `settle_node` stripping its `name`) until the parent's own event folds, `record_assertion` merging within a generation; only the post-batch state is asserted, criterion 9's fixtures reading after the whole batch.

**A RENAMED PATH RETIRES ITS OLD LEAF, decided here.** From criterion 6 one path can refold under another leaf with no twin (gen A `namespace A.B` alone, gen B `class A { B }` alone, both `<file>::A.B`), and from 10 a twin's removal does too (`B` refolded as `A.B`); `ensure_node` patches `name` before `record_assertion` calls `settle_node`, so `settle_node`'s `definition_name` already reads the new leaf. Spec 116's entity arm already reads the prior name through `definition_name` before its `ensure_node` (`defined`, `sqlite.rs:2698`) and hands it to `undo_what_resolved_against` (`:2164-2175`, called at `:2714`) after `ensure_node` has written the incoming `name` and before the new leaf's tier upgrade and `resolve_proofs`, which runs `definition_retired` only when the node no longer defines a name; criterion 15 widens that one function to a prior name differing from the name the node now defines, running `definition_retired` for the old leaf, `settle_node`, its other caller (`:2159`), never changing one `name` to another. The old leaf's convergences thereby undo at any batch position as a cold rebuild folds them (criterion 15's). A rename is any refold of one path under a different leaf, a twin's removal or a cross-generation path reuse, each taking the entity arm's one retirement; a hand-built differing-name pair reaching the arm with no dedup takes the same arm's same outcome, accepted by name (criterion 15's `contextgraph/sqlite.rs` fixture). Criterion 15 lands immediately after 6, where path sharing begins, so interim tree 6 alone holds the stale convergence (the old leaf's cross-file reference `inferred`, its proof landed), accepted by name, no binary of an interim tree being installed (Global constraints).

**ONE EXTENT, decided here.** ONE constructor, `extract::def_of(source, range, kind, name, line)`, in place of spec 116's `unresolved_def` (`extract.rs:162-172`), already the one constructor of every tagged, read-off and impl `Def` (`record_def`, `:194`, the impl loop, `:299`), builds
every `Def` in every language: `line` stays what it is today, a tagged
definition's name row (`tag.span.start.row`, `symbols/extract.rs:267-268`) and a read-off one's row as
its reader gives it (`impl_blocks`' `line`, `:299`), and `end_line`, 1-based like `line`, is the last byte of `range`'s row plus one, `range` the definition's byte range (a zero-length `range` or empty source giving `end_line` = `line`, and `end_line` never below `line` in `def_of`'s output), which a tagged
definition has inside the tags loop (`tag.range`, `:278`, the full parse tree already built, `:243-250`) and
a read-off one from its `ReadOff` range or impl block, so `Def.end_line` is set wherever a `Def` is and no `Def.line` or payload moves
(criterion 5's). `Def.end_line` persists beside `line` with no serde default, since `Def` is the one serde type and a skipped field would leave a live index mixing 0 and real values under one `u32` with derived equality, the stored value read by `Def`'s equality and `sample()`'s round trip, a
stored index rewriting on a line shift as it does for `line`, and an index lacking it loads as absent (GENERATIONS). `show`'s body bound is ONE
grounder function, `extract::locate_extent(source, lang, ts_language, tags_query, name, kind,
line)`, taking the grammar and tags query injected as `extract` takes them (`extract.rs:34-39`): it
runs `extract` over the working-tree bytes and applies three tiers in order: spec 92's first (exact name, kind and line), then name and line with kind ignored, when exactly one production definition matches (so a definition whose kind changed in place since the last ingest still bounds at its recorded line), then spec 92's second as the third (the drifted line re-located by name and kind), to `Def.end_line` over the definitions
that are not `is_test`, as `extract_events` drops them (`events.rs:220`), since the extent resolves
a graph node and the graph holds production ones only (`is_test` read as `extract` gives it: a file whose shebang hides its file-opening `#![cfg(test)]` from `test_regions` is spec 118's, which keeps tests out of every production view, not this spec's), `kind` compared through
`kind_str` in tiers 1 and 3, so a field beside a same-named fn, or a variant inside its same-named enum, never widens
or blocks a match in those tiers, while in tier 2 the recorded line governs by design, so exactly one same-named definition of another kind at the recorded line bounds there even when one of the recorded kind sits elsewhere in the file, the kind-changed-in-place case the criterion pins; from criterion 6 `locate_extent` also takes `path: Option<&str>`, which `print_entity_site`
passes as the rest half of the id's `query::split_id` (none for an id without `::`, matching by name, kind and
line only), and every tier matches it against `Def.path`, so a drifted `T::new` never heals onto `U::new`, an id that carries a path being
matched by it. Tier 1's tie keeps today's rule, the widest `end_line` winning
(`src/cli/graph.rs:417-421`); the dedup's order governs `extract_events` only, so a drifted same-path twin (`#[cfg(unix)] fn x` beside `#[cfg(windows)] fn x`) gets tier 3's ambiguity refusal (`:441-444`) as an ambiguous name does today, accepted. `locate_definition_extent` (`src/cli/graph.rs:402-446`) keeps resolving the
grammar by path through `registry::for_path` (`:409`), so a `.tsx` file keeps its TSX grammar
(`registry.rs:306-341`), and only calls `locate_extent`. What `print_entity_site`
(`src/cli/graph.rs:216-220`) hands `locate_extent` is this block's: `EntitySite.kind` and the `name` attr from `EntitySite.attrs`, never a
cut of the id, since a file id holds a `.` and an impl header id (`impl a::Trait for B`) a `::`; a node at line 0 keeps `source unavailable` (`definition_body`, `src/cli/graph.rs:326-330`), and a nameless node at a non-zero line gets the "no definition named" note with its id in place of the name, in `print_entity_site` ahead of the feature-gated `locate_definition_extent` call (its light-lane stub at `src/cli/graph.rs:453-460`), so both lanes print the same note without calling `locate_extent`.
`definition_extents` and `DefExtent` (`extract.rs:1310-1377`) retire.

**CONTAINS IS STRUCTURE, decided here.** `CONTAINS` runs parent -> child: enum -> variant, struct,
union or variant -> field, impl or trait -> associated item, inline mod or enclosing fn -> item,
the file -> every definition with no parent (a Go method whose receiver type the file does not
define included). The entity arm (`sqlite.rs:2690-2727`) folds the edge from
`code_entity_id(file, parent)`, `ensure_node`-ing that parent bare under the file's identity first,
since a hand-built log or tree 9 can fold a variant before its enum; the parent's own event fills
it. From criterion 16 `extract_events` emits a file's definitions parent before child (A PARENT IS EMITTED BEFORE ITS CHILD), so a parent's own event precedes every child's `ensure_node` and a re-extraction never replaces a
parent's assertion with a bare one. Every production parent folds its own event: an impl is itself a
definition (`impl_blocks`), so `impl Display for T` in a file that does not define `T` gives its
method the impl as parent, and a test definition's children are test definitions, dropped with it. A parent a hand-built log
names without defining it stays the bare node `ensure_node` made, accepted: it
carries no `name`, so it is no member of its file and no candidate, no `CONTAINS` reaches it from
the file, and its children stay reachable through its own `CONTAINS` and by leaf.
`supersede_file_edges` (`sqlite.rs:3257-3270`) also retires `CONTAINS`
whose `from_id` lies in the `<file>::` range, the range it applies to `CALLS` (criterion 9's).
A FILE'S MEMBERS (criterion 14's) are the nodes whose `member_file` is the file, ONE function `query::member_file(node)
-> Option<&str>` giving `node_file` of a `code-entity` node carrying a `name` attr and none for any
other: definitions, nested ones included. Membership reads no edge: a named code entity in the file's id range is a member whatever its `CONTAINS` state. Every reader decides membership by `member_file` alone, `member_sql()` (below) its one SQL spelling; `member_file` answers none for an id whose `split_id` rest half is `None`. A named code entity only an unkeyed assertion holds and no live edge reaches, such as a definition an unkeyed `fresh` re-extraction dropped, is a member of its file and enters every reach through it, accepted by name: only a hand-built log plants it, since from 107 every code-entity `name` is written by `record_assertion` (`sqlite.rs:4052`, its insert at `:4078`) under the ledger entry's identity, never empty, and a node whose keyed assertions lapse loses `name` (`settle_node`, `:2105`) or retires (`retire_unheld_nodes`, `:2064`); `retired_code_entity_count` (`:951`) still counts it retired, counting by its retired `CONTAINS`. A placeholder, a bare parent, a bare caller and a flat id a knowledge assertion still
holds carry no `name`, so none is a member and `member_file` answers none, as today no `CONTAINS` reaches one and `files_reachable` maps an uncontained
entity to no file (`crates/rigger-conductor/src/conductor.rs:13192-13200`).
Every reader of `CONTAINS`, from a literal search of production code for `REL_CONTAINS` and
`"CONTAINS"`, given one disposition (the REPLACED ones criterion 14's):

| Reader (595cbc59) | Disposition |
|---|---|
| `query::member_set`'s file arm (`query.rs:882`), a file card's entities and `reproject`'s (`dash.rs:1682`) file members | REPLACED: the file's members as stated above; a placeholder, bare parent, bare caller, knowledge-held flat id or non-`code-entity` id holding `::` (an `artifact` a finding mention settles) is no member, `node_file`'s artifact arm still giving `show` its file |
| `write_code_neighborhood` (`conductor.rs:13076`), a definition's file | REPLACED by `member_file`, never a `CONTAINS` source, listing only a definition whose `member_file` node the loaded `Graph` holds, with spec 116's call-target filter KEPT (its criterion 4's CALL-SIDE row, A DATA MEMBER IS NO CALL TARGET), so its capped list holds no field, property or declaration; a node it answers none for gets no file line |
| `files_reachable` (`conductor.rs:13184`), an entity's file, its local `file_of` closure (`:13192`, called at `:13227`) | its code-entity arm REPLACED by `member_file`, mapping an entity to its file only when the loaded `Graph` holds its `member_file` node, as the `write_code_neighborhood` row does (no radius change), its `file` arm (a `file` node its own path) KEPT; any other node maps to no file, as an uncontained one does today; its walk crosses `g.edges` alone, so a nested definition reached at the depth bound no longer transits through its file node to the file's other neighbours, accepted by name |
| file and member reach, `Projection::subgraph` (`sqlite.rs:1734`) and `query::neighborhood_of` (`query.rs:918`, behind `neighborhood` and the dash's `graph_json`, `dash.rs:1954`), behind `--around <file>`, the MCP `around` and the `Graph` `write_code_neighborhood` and `files_reachable` receive | REPLACED: a file node entering the reach at hop h, the seed at 0 or one reached mid-walk, also loads its members by the `<file>::` id range at h+1, whatever their nesting, exactly as a file `CONTAINS` hop did (none past the walk's depth), each expanded like any node at h+1, and a member entering the reach at h also loads its `member_file` at h+1, so a nested definition's file and file-mates sit at the hops they sit today, in `subgraph` and `neighborhood` alike; `subgraph` spells `query::member_file` in SQL through ONE adapter function `member_sql()` beside `node_leaf_sql()`, which both of its hops embed, the range load (file to members) and the member-to-file hop, so a placeholder, bare parent, bare caller or knowledge-held flat id enters the reach only through an edge, and a file's neighbourhood holds every definition it holds today and `graph_around_recovery` (`conductor.rs:13056`) stays true |
| `Coupling::from_graph` (`community.rs:166`) | from-agnostic: couples each parent and child, so the file hub couples its top-level definitions and a member couples to its parent, accepted |
| `retired_code_entity_count` (`sqlite.rs:969`), `relabel_owed_communities` (`:2385`), `add_edge`'s relabel debt (`:4173`) | from-agnostic: any live or retired structural edge, whatever its source |
| `supersede_file_edges` (`:3266`) | widened as above |
| the entity arm (`:2722`) | the writer, parent -> child |

**RESOLUTION KEYS ON THE LEAF, decided here.** At HEAD every resolution cuts an id at its first `::`;
the RESOLVER TABLE below is every first-`::` cut and suffix read of an id in production code, from a
literal search of `crates/` and `src/` (test modules excluded) for `split("::")`, `split_once("::")`,
`splitn(2, "::")`, `find("::")`, `rfind("::")`, `simple_name`, `name_suffix`, `file_prefix`,
`strip_prefix` and `'::'`/`"::"`, each site given one disposition; the grounder's term reads (`symbols/grounder.rs:203`, `:239`) split a query's prose term, never a graph id, and are outside it. ONE cut,
`query::split_id(id: &str) -> (&str, Option<&str>)` in `rigger-domain`, finds an id's first `::`
once: its file half is the text before it, or the whole id when it holds none, whatever that text
looks like (an extensionless file a `--language` override ingested included, `Makefile::all`
giving `Makefile`), and its rest half the text after it (`a.rs::` giving `Some("")`), `None` when
the id holds no `::`. The file half is taken as a file unconditionally only for a `code-entity` or `file` node, since the production builders joining an id with `::` are `code_entity_id` (`sqlite.rs:3292`) and, from criterion 3, `placeholder_id` and a
decision, lesson or finding id is the free text its event carries, which may hold `::`; so ONE function, `query::node_file(kind, id) -> Option<&str>` (criterion 13's), gives the file an id names by one kind table: a `code-entity` or `file` node's is the `split_id` file half, none for a `code-entity` whose `split_id` rest half is `None` or whose file half is empty, so A NODE WITH NO `file` HAS NO `line` covers it; a `design-doc`,
`arch-decision`, `handbook-rule`, `rationale` or `artifact` node's is `query::file_of`'s
(`query.rs:119-127`), which takes the same file half and keeps its `#` strip and extension test;
every other kind (a `finding`, `lesson`, `decision`, `unit`, `agent`, `gate`, `stage`, `community` or `concept` node, the table naming all sixteen graph kinds of `contextgraph.rs`) has none, so a dotted id of any of them (`v1.2-choice`, or `gate:lint.sh` from a workflow key holding an extension), which `file_of`'s extension test reads as a file at the base, moves from its directory or `(root)` bucket to its kind, accepted by name; a `community/<res>/<n>` or `concept/<res>/<n>` id ends in a number and moves nowhere. Every `file_of` read the RESOLVER TABLE assigns to criterion 13, `member_file`, `site_of` and `locate` read it. ONE function, `query::leaf_of(id: &str) -> Option<&str>`, gives the rest half less exactly one leading
`contextgraph::PLACEHOLDER_SIGIL` (`%`, the ONE `rigger-domain` constant): `<file>::%x` gives `x`,
`<file>::%%x` `%x`, and an id with no `::` `None`. ONE function, `query::node_leaf(node: &Node) ->
Option<&str>`, gives a NODE's leaf: its `name` attr when it has a non-empty one, whatever it holds (`new` for
`A::T::new`, `A.B`, `a::b`, a C# `operator %`'s `%`, so a named node is never read as a
placeholder), else `leaf_of` of its id; every reader holding a node reads `node_leaf`, and bare `leaf_of` is read only where an id alone is in hand (`resolve_down_hop`'s nameless target and `Coupling::from_graph`'s `canon` over an edge endpoint, `community.rs:148`, its ids nameless by the `bare` map). An empty `name` reads as absent in `node_leaf`, `node_leaf_sql()`, `member_file`, `member_sql()`, `same_file_candidates`, `locate`'s nameless test and every in-memory name-presence test (`Coupling::from_graph`'s `bare` map and definition filter, `community.rs:132-134`, and `defs_by_entity_suffix`'s, `query.rs:154`, which `whole_graph_lens_key` and `reproject_files` read, each reading a non-empty `name`) alike, a bare show's step 1 excepting the nodes whose `node_leaf_sql()` is NULL, and every name-presence test (`named_call_target`'s `name` test, `sqlite.rs:3378`, read at `resolve_down_hop`, `:3481`, `callers_of`, `:3564`, and `reference_tier`, `:3688`, `callers_via_bare`'s `name IS NULL`, `:3534`, `definition_name`, `:2246`) reads `node_leaf_sql() IS NOT NULL` (`callers_via_bare` its negation, spec 116's `OR NOT` call-target disjunct kept), a leafless seed having no cross-file callers.
From criterion 3 `leaf_of` answers `None` for a rest half not beginning with the sigil (on trees
1 and 2 it answers the rest half, every placeholder being unsigiled there), the one decision of
what is a placeholder; every criterion 1 reader reaches it through `node_leaf`, or directly for
`resolve_down_hop`'s nameless target, so a bare parent or bare caller resolves to nothing from 3 and the tree-3 change is that one function's. Its store form is ONE pinned expression, `NULLIF(json_extract(attrs, '$.name'), '')`, index and readers alike: it and `node_leaf` agree on a non-empty `name`, and on an empty or absent `name` the store form is NULL and `node_leaf` is `leaf_of(id)`, so an empty leaf binds nothing, since every SQL reader moved to it requires `name` and a nameless leaf is read in Rust by `leaf_of`. The empty-name rule is criterion 1's and no second mitigation: it is that pinned expression itself, the leaf index's expression and every name-presence reader's one spelling, so `definition_name`'s change is that expression's observable. The expression is returned by ONE function `node_leaf_sql()` in
`rigger-graph-sqlite` beside spec 116's `call_target_sql()` (the domain owns the rule, the adapter
how its store asks), which the leaf index `migrate_indexes` (`sqlite.rs:1662`) adds, its columns project, the pinned leaf expression, then id, and every node-side reader embed verbatim, since SQLite serves an expression index only to the identical
expression (criterion 1's). ONE function, `same_file_candidates(tx, project, file, leaf)`, project-scoped like `reference_tier`, gives the
call targets of a leaf whose id's `split_id` file half is `file`, read from `nodes` by the pinned leaf index filtered to
the `<file>::` id range (a handful of rows per leaf), its tree-1 form keying by leaf only; from criterion 3 (the live-assertion candidate rule, criterion 3's, beside the reference arm, its two observables 3's dropped-definition landing and its proof never naming a node no live assertion holds, the proof-side one planted by a direct `nodes` write since no production fold reaches it: `settle_node` strips an unheld `name` and `definition_retired` re-runs `resolve_proofs`) only a node a LIVE assertion makes a definition is a candidate, each joined by `node_id` through `idx_node_assertions_node(project, node_id)` to a
`live_node_assertions` row (`sqlite.rs:78-82`) whose own attrs carry `name`, the test being that key whatever the row's identity (a knowledge assertion's attrs carry none), so an unkeyed definition is a candidate and one an unkeyed `fresh` re-extraction dropped stays one, accepted by name, the call-target kind read from those rows' attrs, candidates being distinct node ids and a node a call target when any live name-carrying row's kind passes, so a
reference costs the definitions of its leaf in the file, never the file's definitions, so a
knowledge assertion never revives a dropped definition; `resolve_proofs` reads it in place of its
exact `code_entity_id(file, name)` read (`sqlite.rs:3830`): a proof's target is its file's one candidate, else the project's one call target of the leaf (under the same live-assertion test from 3), else none, two same-file candidates
naming none, the proof set being the call-target set spec 116's reader table gives it. The cut between criteria 1 and 2 is NODE SIDE against EDGE SIDE:
criterion 1 is every REPLACED node-side row below, each still deciding which nodes are candidates as
spec 116's criterion 4 has it; a reader that compares an edge's target text at all is criterion 2's
(THE EDGE SIDE BINDS THE LEAF), whatever else it reads, and on tree 1 those readers compare raw text
exactly as today, keeping `name_suffix` and `idx_nodes_name_suffix` until criterion 2 retires both
with them. `definitions_with_suffix` is one function: its candidate read moves to `node_leaf_sql()` at
criterion 1 for every caller, `callers_of` included, whose seed key and edge text stay criterion
2's. The reference arm's target choice does not change here.

| Site (595cbc59) | Side | Disposition | Owner |
|---|---|---|---|
| `Projector::locate`'s fallthrough through `definitions_with_suffix` (`sqlite.rs:877-893`, `:3405`), LOOKUP | node | REPLACED by `node_leaf_sql()` | 1 |
| `resolve_down_hop` (`:3484`), through `definitions_with_suffix` | node | the nameless target's leaf by `leaf_of`, its candidates by `node_leaf_sql()` | 1 |
| `named_call_target`'s name-presence test (`:3378`, read at `:3481`, `:3564`, `:3688`), `definition_name` (`:2246`, the gate through which `settle_node` and the entity arm fire `definition_retired`, by `undo_what_resolved_against`, `:2164-2175`) and `callers_via_bare`'s name-presence test (`name IS NULL`, `:3534`) | node | the empty-name-is-absent rule: each reads `node_leaf_sql() IS NOT NULL`, `callers_via_bare` its negation with spec 116's `OR NOT` call-target disjunct kept | 1 |
| `reference_tier`'s cross-file read (`:3698`) | node | REPLACED by `node_leaf_sql()` | 1 |
| `resolve_proofs`' definition set (`:3795`) and same-file pick (`:3830`) | node | REPLACED by `node_leaf_sql()` and `same_file_candidates`, the live-assertion test criterion 3's | 1 |
| `query::defs_by_entity_suffix` (`query.rs:158`) | node | REPLACED by `node_leaf` | 1 |
| `query::whole_graph_lens_key`'s files arm (`query.rs:578`) | node | REPLACED by `node_leaf` | 1 |
| `reproject_files` (`rigger-dash` `dash.rs:1835`) | node | REPLACED by `node_leaf` | 1 |
| `console::map::build`'s label (`rigger-console` `console/map.rs:332-336`, the `name` attr else `name_suffix`) | node | REPLACED by `node_leaf`, the whole id on `None` | 1 |
| `Coupling::from_graph`'s `simple_name` (`rigger-domain` `community.rs:87-89`, read at `:135`, `:148`) | node | REPLACED by `node_leaf` at `:135` and by `leaf_of` at `:148` (`canon`, an id alone); `simple_name` retires | 1 |
| `migrate_indexes` (`:1666-1669`) | node | the leaf index added | 1 |
| `query::file_of`'s first cut (`query.rs:120`) | file | REPLACED by `split_id`'s file half, its `#` strip and extension test KEPT, no observable change since spec 116's `split_once` already cuts there, so a regression assertion under LANDING TREES' no-change exemption | 1 |
| `cluster_key`'s `file_of` read (`query.rs:96`), the files lens's (`:574`, `:581`) | file | REPLACED by `query::node_file` | 13 |
| `reproject_files`' two `file_of` reads (`rigger-dash` `dash.rs:1840`, `:1861`), `query::card`'s code-entity `file` (`query.rs:1311`), the console map's district (`console/map.rs:269`) | file | REPLACED by `query::node_file` | 13 |
| `callers_of` (`sqlite.rs:3573`) with `callers_via_bare` (`:3533`) | edge | `cur`'s leaf by `node_leaf` over its node row; the edge text bound as THE EDGE SIDE BINDS THE LEAF states; its `named_call_target` call (`:3564`) and `callers_via_bare`'s name-presence test (`:3534`) are criterion 1's row, NOT this one's | 2 |
| the tier upgrade (`:2753`) | edge | the edge text bound to the definition's name | 2 |
| `definition_retired` (`:2273` edge side, `:2276` node side) | edge | node side by `node_leaf_sql()`, edge side bound to the name | 2 |
| `referenced_not_called` (seeds `:3606`, legs `:3621`, `:3635`) | edge | seeds by `node_leaf` over their node rows, a seed with no leaf naming nothing; legs bound to the leaf, or a joined target named it | 2 |
| `query::name_suffix` (`query.rs:134-139`), `idx_nodes_name_suffix` | - | RETIRED with their last callers above | 2 |
| `referenced_not_called`'s calling file (`sqlite.rs:3633`) | file | KEPT: `split_id`'s file half in SQL over a caller id, no leaf read | - |
| `file_prefix` (`:3298-3303`) read by `locate` (`:896`) and `site_of` (`:910`) | file | REPLACED by `query::node_file`; `file_prefix` retires | 4 |
| `owe_relabel_of` (`:2332`), `supersede_file_edges` (`:3264`) | file | KEPT: a `<file>::` range over a file id, no leaf read | - |
| `code_entity_id` (`:3291-3292`) | - | NOT A CUT: it builds the id | 6 |
| `print_entity_site`'s name cut (`src/cli/graph.rs:216-220`) | node | REPLACED by the `name` attr (ONE EXTENT) | 5 |
| `print_entity_site`'s `path` argument | node | `split_id`'s rest half | 6 |

**THE EDGE SIDE BINDS THE LEAF, decided here.** Every edge-side reader (the callers walk, the tier
upgrade, `definition_retired`'s demotion and `referenced_not_called`'s two legs, the referencing
files and the calling files) keeps today's edge expression `substr(to_id, instr(to_id, '::') + 2)`
and its index `idx_edges_live_to_name_suffix`, and compares it `IN (?leaf, '%' || ?leaf)`, two seeks
of the kept index: the first value matches a target spelled `<file>::<leaf>`, as every placeholder
is before criterion 3, the second one spelled `<file>::%<leaf>`, as criterion 3 spells it, so the
binding holds under both spellings and decides nothing about where a call lands; the first value
matches no production placeholder once criterion 3 lands, kept by name as the cost of landing 2
ahead of 3, so an unsigiled nameless target only a hand-built or knowledge-held id spells (`B::new`, no production placeholder being unsigiled from tree 3) is a caller on the up walk while its down hop resolves to nothing through `leaf_of`'s sigil gate, accepted. The leaf bound is the seed's `node_leaf` (`new` for a seed `A::T::new`), or for the tier upgrade and the demotion the
folding or retiring definition's name. `referenced_not_called`'s legs also match a target whose joined node's `name` is the leaf, each leg a
UNION of the two-value seek on the kept edge index and a leaf-index seek on nodes joined to edges by
`to_id`, since a target landed on a nested definition holds its path after
the first `::`, so a reference landed on `<file>::T::new` counts for `new`. Corners: a C# `operator %` (name `%`) binds `%` and `%%`, matching its flat definition and its placeholder,
and a `%`-leading string name (`declare module "%x"`) binds `%x` and `%%x`, so `callers_of` and
`referenced_not_called` on it report leaf `x`'s cross-file callers, and the tier upgrade (`sqlite.rs:2753`) and `definition_retired`'s demotion (`:2273`) of a definition named `%x` promote and demote `B::%x`, accepted; a bare parent's
text (`m::T`) matches only a leaf spelled `m::T`, and no `CALLS` or `REFERENCES` edge targets a bare
parent. `name_suffix` and `idx_nodes_name_suffix` retire with their last callers here (criterion 2's).

**A CALL LANDS ON ITS ONE SAME-FILE CANDIDATE, decided here.** On this base every `EdgeInferred` is
a call (`symbols/events.rs:251-272` carries no reference kind), so every reference resolves under
KIND CANDIDATES' call row (Notes), spec 116's call-target test unchanged (its criterion 4,
`contextgraph::is_call_target` and the adapter's `call_target_sql()` built from
`NON_TARGET_KINDS`), which `same_file_candidates` reads, so a field or `declaration` leaf never makes a
fn leaf ambiguous. The reference arm's target choice is this block's (criterion 3's). A placeholder is
spelled `<file>::%<leaf>` by ONE function, `placeholder_id(file, leaf)`, in `rigger-domain` beside `split_id` and `leaf_of`, reading criterion 1's
`PLACEHOLDER_SIGIL`, `%` beginning no identifier in any registered grammar (a JS or TS private
member begins with `#`), and THE EDGE SIDE BINDS THE LEAF's second value matches it. The domain owns the id format from criterion 1, superseding the rule of the `conductor.rs:13035-13039` comment that the `<file>::<name>` id is the sqlite adapter's private convention (DOCUMENT EDITS). The reference
arm targets the one same-file candidate when there is exactly one, at `extracted`, else `<file>::%<leaf>` at `reference_tier`'s answer for that id (`inferred` when a candidate definition of the leaf exists other than the target id, else `ambiguous`). That candidate definition is one `definitions_with_suffix` admits under `Admitted::CallTargets`, a `nodes` row of a call-target kind carrying the leaf as its name, project-wide; the live-assertion admission (`Admitted::LiveCallTargets`) stays `same_file_candidates`' and `resolve_proofs`' alone, as ARTIFACT OWNERSHIP says, since a `nodes` row no live assertion names is reachable by a direct write only and never by a fold, so the two admissions agree on every folded tree; decided here (criterion 3's, no unit changes). `reference_tier`'s same-file read (`sqlite.rs:3687-3690`, `extracted` when `named_call_target` answers `Some(true)` for the target) retires at criterion 3, the arm deciding `extracted` itself; its one production caller is the reference arm (`:2832`), so no other reader needs it. The landing is defined for a log
whose definitions precede their references within each file, which every production batch is
(`extract_events` emits a file's definitions first, `symbols/events.rs:227-277`) and every
compaction replay keeps; a hand-built reference-first same-file log lands on `<file>::%<leaf>` and
stays there, accepted. `advance_generation` moves the file's `generations` row before `fold_event` runs (`sqlite.rs:1976`),
so gen A's assertions leave `live_node_assertions` (`:78-82`), which `same_file_candidates` reads, and gen A `function
foo(){} foo();` then gen B `foo();` leaves B's call on `%foo`, settled `ambiguous` when no other definition of the leaf remains, by criterion 2's demotion; a dropped definition a knowledge assertion still holds settles `ambiguous` by the same route, `settle_node` stripping its `name`, its kind settling to `artifact` and `definition_retired` firing. Accepted, each by name: two candidates landing on the placeholder (a same-file call naming a PROVIDED trait method (kind `method`) its file also implements, `Trait::m` beside `<T as Trait>::m`, `%m`, while a required trait fn (kind `declaration`, spec 116's `NON_TARGET_KINDS`) beside its one impl lands on the impl at `extracted`; a type beside a same-named variant, `%Foo`; a
class beside its constructor, `N::Foo` and `N::Foo::Foo`, `%Foo`; a container holding a same-leaf
child, `mod x { fn x() {} }`, `%x`); a string name whose content begins with `%` (`declare module
"%x"`) spelling `<file>::%x`, the placeholder id of leaf `x`, and folding onto it; and a seed whose
leaf holds `::` (`declare module "T::new"`) binding the text a reference landed on a nested
`<file>::T::new` holds, so `referenced_not_called` may count that file for it; and from criterion 6 a file holding two call targets of one leaf (two impls each with `new`) no longer lands its in-file test proofs, which landed on the shared flat node before.

**A BARE SHOW FINDS A LEAF OR PATH, decided here.** A bare `show` (criterion 7's) answers in ONE order: the exact id of any node except a nameless `code-entity` or `artifact` whose id holds `::` (today's exact-id match, `locate`, `sqlite.rs:877-893`, its exact read at `:879-882`), the whole query as a leaf, the path form, the file-qualified form, then the exact id of exactly that excepted set, each step taken only on zero answers from the one before; the path form is
a definition whose id ends in `::<query>` (the id suffix compared byte for byte), spec 116's criterion 4 already giving `locate` every node
carrying `name`, criterion 1 already keying it by leaf and criterion 6 giving the nested ids a path
query reaches, so this block owns the path and file-qualified forms: the whole query tried first as a leaf (`namespace A.B`, an impl
header), else a path query looked up by its leaf, the text after its last `::` or `.`, and filtered
on the id suffix `::<path>`, so `show Struct.field` answers the field and `show Kind::Constant` the
variant (a member queried without its generic arguments, `Foo::new` for `<file>::Foo<T>::new`, and a trait-impl member queried without its `<T as Trait>` qualifier (`T::m`) are reachable by leaf or exact id only, a query spelling the full recorded path matching by the suffix rule, a nested dotted leaf such as `N::A.B` misses, and `show a.rs::T::new` misses when only `a.rs::m::T::new` exists, found by `show T::new`, all accepted). A query with a file half that
matches no exact id, no leaf and no path form is tried last as file-qualified, the whole-query step's leaf lookup of its `split_id` rest half
(every node carrying `name`, by `node_leaf_sql()`) filtered to the `<file>::` id range, never `same_file_candidates`, so pre-spec flat ids `a.rs::new`, `a.rs::field` and `a.rs::m`
show `a.rs::T::new`, `a.rs::S.field` and the `declaration` `a.rs::Trait::m`, several listing and none saying none. The file-qualified step runs only for a non-empty rest half; otherwise the file-qualified step answers zero and the order continues to the deferred exact step. The path and file-qualified forms are one mechanism, the leaf lookup filtered on the id, by the suffix `::<path>`
then by the `<file>::` range; a nameless node of graph kind `code-entity` or `artifact` whose id holds `::` is a knowledge-held flat id's bare node (`sqlite.rs:880`), so `show T::new` over an artifact `T::new` beside `a.rs::T::new` answers the definition by the path form. A `--around` seed stays an exact id and a bare name is rejected, never resolved, so no tie across files arises: when the subgraph is empty, `print_around_subgraph`'s note (`src/cli/graph.rs:152`) reads `(nothing found; a seed is an exact id, <file>::<path>, and rigger graph --show <name> resolves a bare name; has rigger run been run yet?)` in place of today's, criterion 7's, and the MCP `around` keeps answering an empty node list, accepted.

**SHOW IS ONE RENDERER, decided here.** `EntitySite` grows `attrs`, which `site_of`
(`sqlite.rs:909-929`) fills from the node's attrs. `site_of` and `locate`'s candidates (`:896`) take `file` from `query::node_file`; `file_prefix`
(`sqlite.rs:3298-3303`) retires (criterion 4's). `kind` keeps its fallback to the graph kind (`:916-920`), an empty `kind` attr falling back too, so
`kind`, like `degree`, is always given; `degree` is a number and always given. ONE function, `contextgraph::site_fields`, gives, `line` and `degree` as numbers (`line` parsed as `site_of` parses it, 0 for absent or unparseable) and every other entry as its recorded value, a verbatim string, the ordered closed list of entries file, line, kind, degree, name, lang, title, doc, by, unit,
summary and partial (`title` and `doc` the knowledge attrs `sqlite.rs:2896` writes, `by`, `unit`
and `summary` a finding's, `:2644`), an empty string field giving no entry and an attr outside the
list nothing (`proven_by` among them, whose entry spec 118 adds); a later field joins the list as one entry; the CLI's two
transforms, the live healed line and a value's newlines collapsed, apply at print only, and the MCP object carries `id` and `site_fields`' entries as keys, compared without order, on `show`, and on every
`around` node less its `summary` entry - the one unbounded attr, so a depth-1 `around` stays a list of sites and a reader wanting a finding's text calls `show` on its id; decided here, criterion 4's. A NODE WITH NO `file` HAS NO `line`, on every
surface at once: `site_fields` gives neither entry, the MCP `show` neither key, the CLI header reads
`  kind K   degree D` with no `site:` part, and the CLI prints no body and no note, never calling
`definition_body` (`src/cli/graph.rs:322-330`); a node with a `file` gives its `line`, 0 meaning
unknown as `site_of` reads it (`sqlite.rs:911-915`), and keeps today's header, body and notes. `rigger graph
--show` prints `show <id>`, then the `file`, `line`, `kind` and `degree` entries through today's `site:` header line
(`print_site_header`, `src/cli/graph.rs:255-267`: the live line, and spec 92's
`(recorded line N, now M)` note when the body healed), then, through `print_entity_site`'s render
path (this block's: its entry lines and its no-`file` branch), one `  <entry> <value>` line per
remaining entry, a value's newlines collapsed to spaces (the MCP key carrying it verbatim), which `tests/common/cli.rs`'s `body_line_count` never counts, then the body; the
MCP `show` gives every entry as a JSON key, `line` the recorded line; the MCP `around`
(`mcpserver.rs:702-707`) renders every node of the loaded `Graph`, queried or neighbour, in place of `{id, kind}`, its `kind` `site_of`'s (the attr kind, else the graph kind, accepted), built with no per-node second read (no `site_of` or `one_hop_degree` per node): `subgraph` loads each `Node`'s kind and attrs whole (`contextgraph.rs:292-296`, `sqlite.rs:1773-1781`), ONE domain function `contextgraph::entity_site(node, degree)` applies `site_of`'s rules to a node, which `site_of` calls over its read row and `around` over each loaded `Node`, and `around`'s degree counts the edges of the merged `Graph` `around` returns (`contextgraph.rs:857-875`), so a node at the depth bound shows a lower degree than `show` gives it, accepted, the observable that proves the no-second-read rule. The CLI `--around` listing (`node <id> <graph kind>`, `src/cli/graph.rs:99`, `:141`) and the dash card (`query.rs:1291`) are outside the renderer, accepted by name; the Goal's "every surface" means `--show`, the MCP `show` and the MCP `around`.

**GENERATIONS AND THE REBUILD.** A generation hashes the batch's bytes (107), not the index version,
and `Def.end_line` is in no payload (PAYLOADS): a batch whose bytes are unchanged keeps its entry, and every other batch (definitions gaining `path`
or `parent`, or a same-path twin's event dropped) re-records under 107's rule for a changed payload. The
index's version bumps once, at its last shape change: `GRAMMAR_TAGS_VERSION` (`symbols/model.rs:181`)
becomes `ts-tags-v4` at criterion 9, where `Def` gains `parent`, so no index labelled v4 exists
before 9; an index lacking `end_line` (from 5) or `path` (from 6) loads as absent before any version
check (`load` deserializes before `SymbolIndex::is_current` filters, `symbols/store.rs:197-199`) and
is rebuilt, and trees 6, 15, 7 and 8 share v3 across criterion 8's `SymRef.enclosing` change, no interim binary
being installed. The projection's version bumps once, after the last fold rule changes:
`PROJECTION_VERSION` (`sqlite.rs:109`) becomes 3 from spec 116's 2 (its criterion 5) at criterion
11, since leaf resolution, the edge binding, the `%` placeholder, the path ids and `CONTAINS` change
what folded rows mean across criteria 1-10, 15 and 16, and one bump means one rebuild of the store pays for
all of them; trees 1-10, 15 and 16 keep version 2 and no interim store exists. Criterion 11's own observable is a version-2 store's stored version reading 3 after the rebuild and
no row at a flat nested-definition id carrying `name`, its fixture planting the flat nested id carrying `name` by a direct `nodes` write beside the
hand-stamped `user_version` 2, as spec 116's version fixtures do, naming no flat id in a knowledge
event, and running the rebuild only; nested ids are criterion 6's. After landing the operator runs `rigger setup`, `rigger graph build`, then `rigger graph communities` and `rigger graph concepts` (`src/cli/graph.rs:654`, `:755`), since `rigger graph build` only ingests the tree (`cmd_graph_build`, `:497-525`) and re-derives neither, so a moved member's community and concept re-attach to its nested id; expected then: one `CONTAINS` per emitted path, below spec 116's extract-seam count (about 6,570) by the same-path twins, and a file holding two call targets of one leaf landing no in-file test proof of it.

**UNIT ORDER AND BASE, decided here.** Launched on rigger-run after spec 116 lands, the binary is
refreshed and the store is migrated (`rigger reset --derived`). Criteria land 1, 2, 3, 13, 4, 5, 6, 15, 7, 8, 14, 9, 16, then 10 to 12, each needing the one before; ARTIFACT OWNERSHIP (Notes) gives every artifact one owner, which is its first
production caller's criterion, and the landing simulation is read off it: no criterion's assertion
touches an artifact whose owner lands after it. 107's batch and rebuild tests compute their
generation through the binary (no test holds a literal `@<hash>` key), so a batch-changing criterion (6, 8-10) edits none of them. A search at 1775b728 of every test calling `extract_events` for an emitted order, count or `fresh` assertion (the `symbols/events.rs` tests, `tests/code_ingest_events.rs:768`, `tests/migration_is_deliberate_periphery.rs:120-281`, `tests/proof_lands_on_the_card_periphery.rs:286`) finds none that criterion 16's emission order or 10's dedup, the first breakers, breaks: each `fresh` assertion pins the first surviving event, which stays a definition, and no fixture holds two surviving definitions of one name, nor a nested definition whose leaf sorts before its parent's (the one nested source those tests write, `#[cfg(test)] mod hidden { pub fn folded() {} }` in `tests/migration_is_deliberate_periphery.rs:274-277`, extracts to the boundary sentinel alone), so criterion 16's sort keys and 10's dedup move no asserted order or `fresh` name. Criterion 15's rename fixture hand-builds `B` at a call-target kind (`function`) on path `A.B`. A criterion changing a signature, payload or struct adapts every caller and constructor in its unit. The audit
gate (`tests/simplification_audit.rs`) holds because each function lands with its first production
caller. Criteria 1 to 3 build nested ids by writing nodes and their assertions directly
(`ensure_node`, `record_assertion`, `sqlite.rs:4052`), criterion 2 a `<file>::%<leaf>` target and
its edge too (`add_edge`), in `contextgraph/sqlite.rs`'s tests, and as in-memory `Graph`s in the
in-memory readers' tests, the payload `path` fold being criterion 6's; criterion 14's `contextgraph/query.rs` and conductor clauses read in-memory `Graph`s holding the named nested node and no file `CONTAINS` edge, and only its `contextgraph/sqlite.rs` clauses use the direct writes (`ensure_node`, `record_assertion`, private to the adapter); criterion 4 asserts over hand-built events, and criterion 5's fixtures include a hand-built nameless node at a non-zero line; criterion 6's `--show` clause queries the exact id `<file>::T::new`; criterion 7 seeds through criterion 6's `path`, by hand-built events carrying it and by a real `rigger graph build`. Criterion 14's re-expressed tests, the three TEST DISPOSITIONS names asserting a node's presence in a reach, fold both the first extraction and the re-extraction under one keyed identity as two generations, the edge-only tests staying unkeyed, the legacy fixture's first fold included, since an unkeyed first assertion is never retracted by a keyed generation. Criterion 1's empty-`name` fixture for its `node_leaf_sql()`/`node_leaf` agreement clause and its `definition_name` settle clause is an id holding no `::` (`x`), whose `leaf_of` is `None` on trees 1 to 3 alike; its `callers_via_bare` clause takes an empty-`name` target that holds `::` (`B::new`, seed a flat `A::new`), whose edge text `substr(to_id, instr(to_id, '::') + 2)` is `new` on trees 1 to 3 alike. Criterion 2's leafless seed is an id holding no `::`, whose `leaf_of` is `None` on trees 1 to 3 alike. Criterion 1's fixtures write a nested
definition directly and fold a cross-file reference AFTER it, one assertion per node-side REPLACED
row of the RESOLVER TABLE, observed through that row's reader, every hand-written placeholder spelled `<file>::%<leaf>`, which resolves on trees 1 to 3 alike; criterion 13's observe one id per `node_file` arm through `cluster_key`; every edge-side reader is criterion
2's, every same-file tier and call landing criterion 3's (which only reads `same_file_candidates`), its trait fixtures a provided `Trait::m` of kind `method` and a required one of kind `declaration`, each beside `<T as Trait>::m`, and its named-`%x` fixture holds no other definition of `x`, so its call settles `ambiguous`; criterion 6's fold fixtures hold `const f = function f() {}` and `const Foo = class Foo {}` at `<file>::f` and `<file>::Foo`
two `const _: () = { fn a() {} }` blocks in one file at `<file>::a` and `var a, b = func() { type T struct{} }(), 1` at `<file>::T`; criterion 9's fold fixtures hold `namespace N; class Foo {}`, `Foo` contained by `N`,
and a Go method contained by its file-defined receiver type; criterion 6's and criterion 9's each also hold one name-filtered container holding a definition, whose path and `parent` skip it. Criterion 6's empty-`path` fixture is a hand-built event whose `path` is empty, folding `<file>::<name>`; criterion 9's are a hand-built event whose `parent` is empty and one whose `parent` names its own path, each contained by the file. Criterion 1 asserts `locate`'s fallthrough by leaf; criterion 7 adds only the path and file-qualified forms, after the
ids it queries exist. LANDING TREES (Notes) names the tree each criterion is red on and the tree it
is green on; no criterion asserts the opposite value of an observable an earlier one asserts.
Criteria 1 and 2 assert no call landing: trees 1 and 2 land every call exactly as the base does, and 3 owns every landing observable.
Between 6 and 8 a call inside a nested definition folds its `CALLS` from a bare `<file>::<leaf>`
node, which no assertion of 6, 15 or 7 reads and no installed binary runs, accepted: no test in the
tree asserts a fold-level edge from a nested caller (TEST DISPOSITIONS), so the caller's path stays
criterion 8's. Criterion 3's dropped-definition clause observes the folded edge's `to_id` (`<file>::%foo`, never the held flat id), red on 2 (LANDING TREES), unit 3 adding the live-assertion join to `same_file_candidates` and fixing the arm's choice of reader (`same_file_candidates`, never `nodes`);
its fixture's call is the first event of generation B, since `retire_unheld_nodes` strips `name` from the dropped node after a new generation's first event. On trees 1 and 2 a cross-file reference to a `%`-leading leaf (a C# `operator %`, a TS member
`"%x"`) folds `<file>::%` or `<file>::%x` and resolves by leaf `x`, the empty leaf of `<file>::%` binding nothing, accepted. Pairs read off the table, each "yes": 1 before 2, 3, 6 and 7 (each reads a leaf, never a cut id); 2 before 3 (3's `%`
placeholders are read by the edge-side readers through 2's second value); 1 before 13 (`node_file` reads `split_id`); 13 before 4 (`site_of`, `locate` read `node_file`); 13 before 14 (`member_file` reads it); 14 before 9 (9 drops the file's `CONTAINS` to a nested definition, a member by 14's range); 4 before 5 (5 hands
`locate_extent` the `name` attr from 4's `EntitySite.attrs`); 6 before 15, 7, 8, 9 and 10 (each reads
`Def.path`, 15 refolding one path under another leaf); 9 before 16 (16 sorts by 9's `Def.parent`); 16 before 10 (16's sort orders 10's winners); 9 before 10 (the winner's parent); 11 asserts on a store 1-10 and 14 to 16 folded.

**CONSTRAINTS WALK, decided.**
- *Empty:* a node with no `file` has no `line` (SHOW IS ONE RENDERER); a file-level definition has
  no `parent` and the file's `CONTAINS`; a nameless code entity is no member of a file.
- *Repeated:* a batch re-emitting the same paths re-asserts the same ids.
- *Reverted:* a twin removed then restored re-derives its winner each generation (ONE PATH, ONE
  EVENT); a TS class member flipping between a literal value (`C.f`, a field) and an arrow function
  (`C::f`, a method per spec 116) moves its id on each such edit, accepted by name beside the
  flat-to-nested move (Existing data): the separator is its kind's, and a knowledge link on the old id stays on the bare node Existing data describes.
- *DROPPED:* a removed definition's `CONTAINS` retires with its file's next generation (widened `supersede_file_edges`), and its node loses `name` or retires, leaving every member set and range load (CONTAINS IS STRUCTURE).
- *Fold order:* among orders that keep each batch's `fresh` head first and each file's definitions
  before its references, no order changes a live edge's existence, tier or `source`, nor a node's
  existence; a reference-first same-file log is accepted (A CALL LANDS ON ITS ONE SAME-FILE
  CANDIDATE), and an unkeyed pair sharing one id takes its last-folded kind as spec 116's criterion 5 decides, excepted.
- *Concurrent ingests:* the reference arm reads the live assertions inside the folding batch's one transaction, so another identity's batch is seen whole or not at all. `locate`'s steps share no read transaction, a tear accepted by name: they run under the connection mutex in-process (`sqlite.rs:878`), so only a fold another process commits between its steps can answer from two generations, the existing "candidate vanished between reads" error being that case's visible form today, and the next query heals it.
- *Crash-resume:* the landing runs in the batch's one transaction.
- *Cold start:* ids, `CONTAINS`, landings and placeholders are `graph.db` projections replayed in generation order.
- *Existing data:* 107's migration sheds every derived row, keyed or unkeyed, so no unkeyed
  pre-spec `CodeEntityExtracted` remains on the operator's store; every new payload field is
  optional, so a hand-built unkeyed one folds a flat definition, accepted, while a pre-spec entry of a changed batch folds nothing at the
  rebuild (107's rule) and its file returns at the next ingest; a `#`-suffixed id's `site:` part changes from the whole id to its file, and a
  file-less knowledge node's `show` loses today's `site: <id>:0` header part and its `source
  unavailable` note (SHOW IS ONE RENDERER); an extensionless `artifact` path (`Makefile`, `Dockerfile` touched before code ingest) reads as file-less until a `file` assertion promotes it, accepted by name. Post-rebuild, a flat id a decision, lesson, finding or design link names stays live, held by that knowledge assertion (`settle_node`'s live assertions), as a bare node of the kind
  `settled_kind` gives them (`sqlite.rs:2130-2141`; `artifact`, the kind every mention arm asserts, `:2489`, `:2614`, `:2649`, `:2945-2946`) without the definition's keys, its `GOVERNS`/`ABOUT`/`SPECIFIES` edge not reaching
  the nested definition; a `CommunityAssigned` or `ConceptRealized` event naming a moved member is
  recorded by `attach` (`sqlite.rs:2429-2465`) against that bare node, and the next community
  derivation re-labels the nested one; both accepted, since `PROMOTING_KINDS` (`:4024-4034`) changes
  a node's kind and never its id, and an alias written by the definition arm would catch only
  mentions folded after it, so fold order would decide. Only nested definitions (members, variants, associated items) move ids, and a seed stays an exact id (`contextgraph::around`, `contextgraph.rs:855-857`, behind `--around` and the MCP `around`; the dash calls view, `src/cli/dashboard.rs:554`), so a lesson's flat seed walks from the knowledge-held bare node, accepted by name.

**STATE PLACEMENT.** Ids, `CONTAINS`, landings and placeholders are `graph.db` projections of the
ledger and the tree; `Def.path`, `parent` and `end_line` are computed by `extract` from one file's
bytes; the renderer reads `graph.db` and the working tree only.

## Notes (non-criteria)

ARTIFACT OWNERSHIP. Owner = the criterion of the artifact's first production caller (for a field,
table or constant, its first production reader or writer); "-" = not retired.

| Artifact | Owner | First production caller | Tree it first exists on | Retired by |
| --- | --- | --- | --- | --- |
| `query::split_id`, `query::leaf_of`, `query::node_leaf`, `contextgraph::PLACEHOLDER_SIGIL`, `node_leaf_sql()` and the leaf index, `migrate_indexes` adding it, `query::file_of` reading the cut, `same_file_candidates` and `resolve_proofs`' same-file pick keyed by leaf, `named_call_target`'s, `definition_name`'s and `callers_via_bare`'s name-presence test reading `node_leaf_sql() IS NOT NULL`, every node-side REPLACED row of the RESOLVER TABLE | 1 | `Projector::locate`, `resolve_down_hop`, `reference_tier`, `resolve_proofs`, `defs_by_entity_suffix`, `whole_graph_lens_key`, `reproject_files`, `console::map::build`, `Coupling::from_graph`, `file_of`, `settle_node`, the projection open | criterion 1 | - |
| `community::simple_name` | - | - | base (116) | 1 |
| the edge-side binding of a leaf and its sigil-prefixed form over `idx_edges_live_to_name_suffix`, the seeds' leaf at `callers_of` and `referenced_not_called`, every edge-side row of the RESOLVER TABLE, `migrate_indexes` dropping the node name-suffix index | 2 | `callers_of`, the definition arm's tier upgrade, `definition_retired`, `referenced_not_called`, the projection open | criterion 2 | - |
| `name_suffix`, `idx_nodes_name_suffix` | - | - | base (116) | 2 |
| `placeholder_id` and the `%` spelling, `leaf_of`'s sigil gate, the live-assertion candidate rule in `same_file_candidates` and `resolve_proofs`, the reference arm's target choice, `reference_tier`'s same-file read retired | 3 | the reference fold arm | criterion 3 | - |
| `EntitySite.attrs`, `contextgraph::site_fields`, the empty-field rule, the no-`file` rule, `print_entity_site`'s render path, `contextgraph::entity_site` with `site_of` and `around` through it | 4 | `cmd_graph_show`, `print_entity_site`, `tool_graph` | criterion 4 | - |
| `file_prefix` | - | - | base (116) | 4 |
| `extract::def_of`, `Def.end_line` with its zero-length rule, `extract::locate_extent` with its recorded-line tier second, the kind and `name` attr `print_entity_site` hands it, the nameless-node note | 5 | `extract`, `locate_definition_extent`, `print_entity_site` | criterion 5 | - |
| `unresolved_def`, `definition_extents`, `DefExtent` | - | - | base (116) | 5 |
| `Def.path`, `CodeEntityExtracted.path` and the id built from it, the enclosing-path prefix with its strict containment, impl ids, the Go receiver path, `extract_events` writing `path`, the named function or class value's path, `locate_extent`'s `path` parameter matching `Def.path`, the empty-`path` rule | 6 | `extract`, `extract_events`, `print_entity_site`, `locate_definition_extent` | criterion 6 | - |
| bare `show` by path, the file-qualified form as the leaf lookup filtered to the file's id range, the nameless exact-step deferral, the `--around` not-found note | 7 | `locate`, `print_around_subgraph` | criterion 7 | - |
| `SymRef.enclosing` and `EdgeInferred.caller` as a path | 8 | `enclosing_def` | criterion 8 | - |
| `Def.parent`, `CodeEntityExtracted.parent`, parent `CONTAINS` with its bare parent `ensure_node`, widened `supersede_file_edges`, `GRAMMAR_TAGS_VERSION` `ts-tags-v4`, the empty-`parent` and self-parent rules | 9 | `extract_events`, the entity fold arm, `SymbolIndex::is_current` | criterion 9 | - |
| parent-before-child emission, the stable sort by parent chain, leaf and path | 16 | `extract_events` | criterion 16 | - |
| the same-path rule, one event per path, the named value's single event | 10 | `extract_events` | criterion 10 | - |
| `undo_what_resolved_against`'s differing-name arm, `definition_retired` for a renamed definition's old leaf | 15 | the entity fold arm | criterion 15 | - |
| `PROJECTION_VERSION` 3 | 11 | the projection open | criterion 11 | - |
| `query::node_file` and its kind table, every REPLACED `file_of` read of the RESOLVER TABLE | 13 | `cluster_key`, `whole_graph_lens_key`, `reproject_files`, `card`, `console::map::build` | criterion 13 | - |
| the member rule, `query::member_file`, `member_sql()` at both `subgraph` hops, the REPLACED `CONTAINS` readers | 14 | `member_set`, `write_code_neighborhood`, `files_reachable`, `subgraph` | criterion 14 | - |

UNIT SIZE (added lines, code and tests, estimated as 107 did): 1 ~650; 2 ~200; 3 ~350; 4 ~550; 5
~390 (`def_of` in place of `unresolved_def` and `locate_extent` ~120, `record_def`'s and the impl loop's calls passing the range ~20, `definition_extents` removal net negative, tests ~250); 6
~600; 7 ~200; 8 ~200; 9 ~200; 10 ~60; 11 ~100; 13 ~150; 14 ~200; 15 ~60; 16 ~80. Every unit is under the 3,000-line cap.

KIND CANDIDATES. A call: the definitions spec 116's `contextgraph::is_call_target` (its criterion 4) admits. The table binds references only; a bare `show` query takes every code-entity kind.

PAYLOADS. `CodeEntityExtracted { .., path?, parent? }`, `extract_events` writing `path` only when it
differs from `name` and `parent` only when present; both serde-defaulted and omitted when absent,
since the log replays old payloads. `Def { .., path, parent?, end_line }`: `end_line` and `path` always written with no serde default (GENERATIONS), `parent` defaulted and omitted when absent. An empty `path` or `parent` reads as absent, and a `parent` naming the entity's own path folds the file's `CONTAINS`.

TEST DISPOSITIONS, every test named read at 595cbc59, each search run at 1775b728 and over every test spec 116's criteria name.

| Test | First broken by | Disposition |
|---|---|---|
| tests naming a placeholder id `<file>::<leaf>`, among them `a_cross_file_calls_edge_upgrades_ambiguous_to_inferred_with_its_references_twin` (`contextgraph/sqlite.rs:8445`, `src/a.rs::G` at `:8484`) | 3 | re-expressed as `<file>::%<leaf>` |
| a search of every `#[test]` body for a hand-built log folding a same-file reference before its definition and reading the definition id as the edge target (each body whose first reference event precedes its first definition event: 13, among them `the_code_graph_is_rebuildable_from_the_log_re_deriving_identical_nodes_and_tiered_edges`, `contextgraph/sqlite.rs:11092`) | 3 | none found: each such reference is cross-file, test evidence, or names a leaf no later event defines |
| spec 116's criterion 4 test of a same-file call naming a field's leaf | 3 | kept: its down hop and tier answer are unchanged; an assertion over that edge's `to_id` re-pinned from the field's `<file>::<leaf>` to `<file>::%<leaf>`. Spec 116's criterion 5 tests (references folded after both definitions) are not broken |
| `additive_indexes_exist_and_the_pinned_name_suffix_query_uses_its_index` (`sqlite.rs:11234`), `tests/graph_additive_indexes_persist.rs` `:49` and `:153` | 2 | re-expressed: the leaf index and `idx_edges_live_to_name_suffix` exist, `idx_nodes_name_suffix` does not, the pinned leaf query, the two-value edge seek and `referenced_not_called`'s leaf-index node seek use their indexes |
| calls-walk, tier and proof tests | 1 | kept: a search of every test for `calls_down`, `calls_up`, `callers_of`, `--calls`, `proven_by`, `resolve_proofs`, `proofs` or `proof_evidence` finds these 39, the conductor, CLI and harness hits naming the words in prose only excepted: in `dash.rs` `card_of_a_membership_less_entity_has_no_community_and_no_line`, `card_of_a_proven_code_entity_carries_proven_by_and_proof_evidence`, `card_of_an_unproven_code_entity_has_proven_by_zero_and_no_evidence`; in `contextgraph/sqlite.rs` `calls_down_walks_the_execution_path_as_a_layered_deduped_dag_with_a_back_edge`, `calls_down_follows_a_single_candidate_hop_but_marks_a_multi_candidate_one_a_sorted_frontier`, `calls_down_leaves_a_zero_candidate_cross_file_callee_a_terminal_leaf_not_a_frontier`, `calls_up_walks_the_call_sites_as_a_layered_deduped_dag_and_lists_referenced_but_not_called`, `calls_up_marks_an_ambiguous_cross_file_caller_a_frontier_and_does_not_ascend_it`, `additive_indexes_exist_and_the_pinned_name_suffix_query_uses_its_index`, `a_same_file_test_reference_increments_proven_by_and_records_its_evidence`, `two_test_references_accumulate_proven_by_to_2_with_both_evidence_entries`, `an_is_test_edge_creates_no_file_node_and_no_edge_of_any_kind`, `a_cross_file_test_reference_resolves_by_name_when_the_definition_already_exists`, `an_unresolvable_test_reference_is_staged_and_reconciled_once_its_definition_later_folds`, `reconciliation_is_a_no_op_when_nothing_is_pending_for_a_newly_defined_entity`, `re_extracting_an_edited_file_does_not_silently_drop_an_unrelated_files_accumulated_proof`, `an_ambiguous_same_named_pair_never_gets_confident_credit_through_either_resolution_path`, `re_extracting_a_test_file_supersedes_rather_than_accretes_or_strands_its_own_evidence`, `a_files_evidence_dropping_to_zero_retracts_its_own_stale_contribution_via_the_empty_boundary_sentinel`, `the_empty_boundary_sentinel_never_resolves_records_or_stages_anything_for_its_empty_name`, `the_empty_structural_boundary_sentinel_creates_no_node_and_no_edge_of_its_own`, `a_retired_node_that_returns_keeps_the_proof_another_file_recorded_on_it`; in `symbols/events.rs` `ingesting_a_fixture_with_product_and_test_code_graphs_only_the_product_and_lists_no_test_file`, `a_product_entity_referenced_by_two_tests_carries_proven_by_2_and_renders_on_its_card`; `calls_down_surfaces_a_multi_candidate_hop_as_a_marked_frontier_and_never_descends_it`; in `tests/compaction_generations_periphery.rs` `a_fact_that_returns_after_a_generation_dropped_it_is_dated_from_its_return`, `a_proof_on_a_dropped_definition_lands_on_the_next_definition_of_its_name`, `proofs_list_in_recorded_order_and_a_dropped_last_proof_leaves_no_proof_attrs`, `a_tests_proof_survives_its_entitys_retirement_and_returns_with_it`, `an_open_owes_the_rebuild_until_it_is_paid_writes_nothing_and_an_unfolded_file_owes_none`; in `tests/proof_lands_on_the_card_periphery.rs` `proof_lands_through_the_public_pipeline_independent_of_the_implementers_own_fixture`, `forward_referenced_evidence_through_the_public_pipeline_is_reconciled_once_its_definition_later_folds`, `cross_file_proof_survives_a_real_reextraction_of_the_defining_file`, `a_real_reextraction_of_a_test_file_supersedes_rather_than_accretes_or_strands_its_own_evidence`, `a_real_ambiguous_same_named_pair_never_gets_confident_credit_through_either_resolution_path`, `a_deleted_test_reference_retracts_its_stale_proof_through_the_real_pipeline`, `a_reference_free_tests_dir_files_first_extraction_creates_nothing_and_leaves_no_residue`, `the_served_graph_card_route_carries_proven_by_and_proof_evidence_as_a_real_json_array`, `the_served_graph_card_route_omits_proof_evidence_but_still_serves_proven_by_zero_for_an_unproven_entity`; every id they fold flat on tree 1, so each leaf is its `name` and each answer unchanged; of them a search for a direct `INSERT INTO nodes` finds two, `additive_indexes_exist_and_the_pinned_name_suffix_query_uses_its_index` (a nameless node for the index plan) and `an_open_owes_the_rebuild_until_it_is_paid_writes_nothing_and_an_unfolded_file_owes_none` (an older store the rebuild refolds), neither planting a proof's definition |
| the same 39 calls-walk, tier and proof tests, and every test folding an `EdgeInferred` after a direct `nodes` write of a named code entity in the same file | 3 | kept: the two direct `INSERT INTO nodes` above plant no proof's definition, so the live-assertion test changes no proof answer; the wider search (every test writing `nodes` through `INSERT INTO nodes` or `ensure_node` and folding an `EdgeInferred`) finds those two alone, each writing a nameless row (`attrs` NULL), so no reference lands on a definition only a direct write planted |
| `tests/graph_show_surface.rs`, `tests/graph_show_periphery.rs`, `tests/graph_show_staleness.rs` | - | kept, but `graph_show_periphery.rs:365` and `:559`, which the nested flat id row takes: the `site:` header line is unchanged for a node with a file (`graph_show_staleness.rs:66-69`'s `gone.rs:7` included) and an entry line is no body line; a search for `source unavailable`, `:0` and negated `contains` finds only code nodes with a file (`graph_show_periphery.rs:151`, `:170`, `:216`) and doc-comment absences (`:346`, `:424`), none over a file-less node or an attr now printed |
| `cluster_key_honors_the_boundary_edges_of_the_names_a_file_predicate` (`tests/dash_graph_exploration_fold.rs:79`) | 13 | re-expressed: its six extensionless `file` ids (`.gitignore`, `config/.gitignore`, `src/.env`, `notes/todo.`, `src/utils/Makefile`, `src/nested/`) fold to their directory, `.gitignore` to `(root)`, a `file` node naming its file whatever its last segment; the rest of a search of every test for `cluster_key` (`dash.rs:5781`, `query.rs:2132`, `dash_graph_exploration_fold.rs:45`, `:197`, `:210`) and of the `clustered_overview`, `cluster_detail` and lens tests' ids finds only code entities, docs, rationales, agents, communities and concepts whose bucket stays, kept; a search at 1775b728 of every test's `decision`, `finding`, `lesson` and `unit` node ids for a dotted id finds none among 631 such literals, and the same search over `agent`, `gate`, `stage`, `community` and `concept` ids (553 literals spelled `gate:`, `stage:`, `agent:`, `community/` or `concept/`, and every literal `file_of` reads as a file within three lines of one of those kinds) finds none but three assertion messages, so no test's bucket moves to its kind |
| MCP `show` and `around` tests: `mcpserver.rs` `lookup_surface_rigger_graph_show_resolves_via_the_locate_trait_method` (`:2156`), `lookup_surface_rigger_graph_show_lists_ambiguous_candidates` (`:2272`), `spawn_bound_surface_still_serves_peers_ground_and_graph` (`:2544`); `tests/relocated_worktree_store_resolution_periphery.rs:248`, `tests/cli.rs:25553`, `tests/cli.rs` `mcp_tool_call_edges_report_errors_for_unknown_tool_and_missing_required_arguments` (`:26920`), `mcp_rigger_graph_show_resolves_a_seeded_entity_and_reports_none_for_an_unknown_one` (`:27026`) | - | kept: each asserts `site`'s `id`, `file`, `line` or `kind`, a candidate's `file` or an error, all kept; none pins an `around` node's `{id, kind}`. Search: every test calling `rigger_graph` with `show` or `around` |
| hand-built `--show` fixtures printing a body: every `apply_code_entity` in `tests/graph_show_periphery.rs`, `tests/graph_show_staleness.rs` and `tests/graph_show_surface.rs` | 5 | kept: a search comparing each fixture's kind string with `kind_str` of what `extract` gives its source finds `function` for every one (a file-level or nested Rust `fn`, a Python `def` in a `def`, a JS `function`; `notes.txt` has no grammar), and no fixture source holds an `is_test` definition, so each body prints as today |
| a search of every test calling `locate`, `--show` or the MCP `show` with an id holding `::` whose node is nameless while a named same-leaf node sits in its file range (47 tests calling them) | 7 | none found: no such test holds a second literal id in the queried id's file range with the queried leaf, so no exact answer moves behind the leaf, path or file-qualified steps |
| `definition_extents` tests (`extract.rs` tests, its helper `extent_of` at `:2592`) | 5 | re-expressed over `extract`'s `Def.end_line` |
| spec 116's criterion 10 extent assertion (`symbols/registry.rs`: `z = 1` bounded by the whole assignment, `x` given none) | 5 | re-expressed: `x` bounded by its own identifier's row and `z = 1` by the whole assignment |
| spec 116's caller assertions naming a nested enclosing definition (criterion 7's field type reference, criterion 9's `constructor` body call and `property_signature` type reference) | 8 | re-expressed at the caller's path (`S.f`, `C::constructor`, `I.p`); 116's other criteria assert top-level callers, flat ids from hand-built events or leaves, which no 119 criterion changes |
| `save_load_roundtrips_and_bytes_are_stable` (`symbols/store.rs:241`, its assert at `:246`) and `save_publishes_atomically_and_leaves_no_temp_residue` (`:305`, asserts at `:311`, `:325`), comparing a loaded index with `sample()` | 5 | kept: `sample()` builds each `Def` with an `end_line` from 5, its `path` from 6 and `parent: None` from 9, each round-tripping |
| the legacy-index tests loading hand-written JSON through `tests/common/symbol_index.rs`'s `legacy_file` (`:255`): `a_pre37_persisted_index_loads_folding_references_caller_less` (`tests/symbol_ref_caller_attribution.rs:84`), `a_pre86_persisted_index_with_no_is_test_key_loads_defaulting_every_item_to_false` (`tests/code_entity_test_exclusion_periphery.rs:301`) and every caller of `legacy_module_def` (`tests/code_entity_test_exclusion_periphery.rs:352`) | 5 | kept, through the helper: each legacy JSON's definitions carry no `end_line` (from 5) or `path` (from 6), so `serde_json::from_slice` fails on the missing field, `load` answers `None` and `legacy_file`'s `expect` panics, never reaching its assertions. Each test's point is one OTHER field's serde default (`enclosing` on a reference, `is_test`, the module flags), which a missing field would mask, and an index lacking either is absent by design (GENERATIONS). So `legacy_file`, which already stamps the current `GRAMMAR_TAGS_VERSION` so that it proves the per-field default alone, also stamps each definition lacking `end_line` with 0 and lacking `path` with its `name`, the shape a current index holds; every fixture JSON and assertion stays as written. The same helper file's `def()` (`:22`) gains `end_line: 0` at 5, `path` (its name) at 6 and `parent: None` at 9 |
| tests naming a nested definition's flat id over real extraction, among them `a_cfg_test_impl_block_excludes_its_methods_through_the_public_api` (`tests/code_entity_test_exclusion_periphery.rs:1447`, `widgetimpl.rs::ordinary_method` at `:1476`, the absent `helper`/`it_works` ids at `:1458`) and its inner-form twin (`:1517`, `:1528`), `:600`, `:796`, `:885`, `:974`, `:1026`, `proof_lands_on_the_card_periphery.rs:154`, and `graph_show_periphery.rs:365` and `:559`'s hand-built nested ids, folded at 6 with `path` | 6 | re-expressed at the path id (`widgetimpl.rs::Ordinary::ordinary_method`) |
| `a_reference_is_attributed_to_its_enclosing_definition` (`symbols/extract.rs:1574-1663`, `enclosing == Some("inner")` for `fn outer() { fn inner() { G(); } }`), `rust_impl_blocks_are_definitions_linked_to_their_trait_and_type_and_path_calls_are_references` (`extract.rs:1425`, `("helper", 3, Some("append"))`), the only nested callers a search of every test asserting an `enclosing` or `caller` value finds; the other ten (`a_caller_carrying_reference_event_round_trips_through_the_fold_deserialize_path`, `a_source_file_extraction_emits_events_the_fold_turns_into_a_code_graph`, `extract_events_lowers_each_reference_to_an_edge_carrying_its_enclosing_caller`, `a_file_that_extracts_to_nothing_still_stamps_an_empty_structural_boundary`, `extract_events_skips_is_test_items_and_the_fresh_boundary_lands_on_the_first_survivor`, `a_domain_allowlist_entry_that_moved_or_vanished_is_reported`, `a_pre_round9_persisted_index_with_no_enclosing_inline_module_path_key_loads_defaulting_to_none`, `no_committed_candidate_comes_from_a_known_out_of_line_test_file`, `a_caller_attributed_reference_serializes_and_reloads_its_enclosing_name`, `extractor_attribution_survives_the_build_index_save_load_pipeline`) pin a top-level fn, an impl header or no caller, unchanged | 8 | re-expressed as the caller's path (`outer::inner`) |
| a real-extraction test whose same-file call names a leaf two nested definitions now share | - | none in the tree: a search of every test string literal holding two `impl` or `class` bodies, or an `impl` beside an `enum`, for a repeated `fn`, `def` or `func` name finds `extract.rs:1427` and `:1535` (extract-level, no fold) and `tests/simplification_audit.rs:10648-10853` (audit fixtures) |
| a search of every test for `subgraph`, `--around`, `member_set` or `files_reachable` after an unkeyed hand-built or real-pipeline `fresh` re-extraction, read test by test, those asserting a node's presence in a reach | 14 | re-expressed: `re_extracting_a_changed_file_supersedes_its_removed_symbols_end_to_end` (`tests/code_ingest_events.rs`), `a_whole_tests_dir_files_legacy_entity_is_retired_through_the_real_extract_events_and_fold_seam` and `a_product_file_rewrapped_entirely_into_cfg_test_retires_its_prior_entity_through_extract_events_own_end_of_function_boundary` (`tests/migration_is_deliberate_periphery.rs`), each dropped definition keeping `name`, so `member_file` makes it a member, re-expressed by folding the first extraction and that re-extraction under one keyed identity as two generations (UNIT ORDER), the dropped definition then losing `name` and leaving the reach, the asserted absence unchanged |
| the same search's tests asserting edges, a kept node or a card | - | kept, unkeyed, not broken by 14: `re_extraction_supersedes_a_files_prior_structural_edges_without_deleting_them`, `re_extraction_supersedes_a_files_prior_calls_edges`, `pruning_superseded_edges_leaves_the_live_grounding_subgraph_exactly_unchanged` and `re_ingesting_a_store_that_already_holds_test_entity_nodes_retires_them_through_supersession_and_leaves_product_entities_unchanged` (`contextgraph/sqlite.rs`), `a_refs_only_files_re_extraction_supersedes_via_the_reference_batch_boundary` and `a_re_extraction_supersedes_only_its_own_files_edges_not_another_files_reference` (`tests/code_ingest_events.rs`), `a_real_reextraction_of_a_test_file_supersedes_rather_than_accretes_or_strands_its_own_evidence` (`tests/proof_lands_on_the_card_periphery.rs`) and `extended_prune_reclaims_superseded_edges_at_the_boundary_leaving_the_live_subgraph_unchanged` (`tests/graph_superseded_prune.rs`): membership reads no edge, so each live-edge, kept-node, card and before-and-after comparison holds; the unkeyed real-pipeline tests of `tests/code_entity_test_exclusion_periphery.rs` reach a nested method through its impl's `CONTAINS` within their depth 3 |
| a search of the 35 tests calling `neighborhood`, `neighborhood_of`, `graph_json` or `card` for a re-extraction or a superseded edge | 14 | kept: it finds `neighborhood_bounds_by_depth_follows_both_directions_and_skips_invalidated_edges` (`dash.rs:7151`) and `a_plain_neighborhood_omits_every_additive_call_field` (`:9258`), whose ids (`a` to `d`) hold no `::`, so `member_file` names no member, and `tests/proof_lands_on_the_card_periphery.rs:198`, `:416`, `:488`, `:591`, `:686` and `:745`, whose `card` reads proof over a store-loaded `subgraph`, never membership; none asserts an unkeyed-only named node absent from an in-memory read |
| `member_set_of_a_file_counts_only_its_own_live_contains_edges` (`contextgraph/query.rs:2100`, nameless `plain` members through live, superseded and wrong-rel `CONTAINS`) | 14 | re-expressed: `src/w.rs::w`, `::y` (superseded `CONTAINS`) and `::z` (wrong rel) carry `name` and are all three members, membership reading no edge; `src/x.rs::x`, named in another file's range, and a nameless in-range node are not |
| test helpers calling a retired cut: `community.rs:551-575`'s `graph()` (`simple_name`), `dash.rs:5864`, `:6290`, `:6500`, `:6945` (`name_suffix`) | 1, 2 | re-expressed through `node_leaf` and the edge-side leaf in the unit retiring the cut |
| tests asserting `CONTAINS` from the file to a method, variant or nested item | - | none in the tree: of the tests naming `REL_CONTAINS` or `"CONTAINS"`, the four folding real extraction (`tests/code_ingest_events.rs:290`, `symbols/events.rs:846`, `:1668`, `tests/code_entity_test_exclusion_periphery.rs:559`) contain top-level fns only and the rest fold hand-built events carrying no `parent` |
| a test asserting a fold-level `CALLS` edge from a nested caller | - | none in the tree: a search of every test fixture literal for a call inside an impl method, a class method, a Go receiver method, a Python method or a nested fn finds `extract.rs:1427` and `:1582` (extract-level `enclosing`, criterion 8's row), `grounder.rs:1516`, `:1520` (ranking), `tests/code_entity_test_exclusion_periphery.rs:1421` (a test-only impl) and `tests/simplification_audit.rs:10678` (an audit fixture), none folding a nested caller's edge |
| spec 116's version fixtures: the tests its criterion 5 re-points through `PROJECTION_VERSION`, the `PROJECTION_VERSION - 1` and `user_version = 0` fixtures and its criterion 13's hand-written `user_version` 1 store | - | kept: each reads `PROJECTION_VERSION`, never the current value as a literal, or holds an older literal that still owes its rebuild at 3 (spec 116's search: every `.rs` file for `PROJECTION_VERSION` and `user_version`) |
| 107's batch, rebuild and sink tests | - | kept: a search of every `.rs` file for a literal `@<hash>` key finds only hand-written keys no test compares with a minted one (the `@h1`-style keyed fixtures, `@abc123` in `dedup_seeding_periphery.rs:147-148`, `@deadbeef` in `rigger-domain` `ingest.rs:461` and `tests/published_content_key_split_periphery.rs:84`), each batch test computing its generation through the binary |

DOCUMENT EDITS, each to the criterion whose change makes the text false, from a literal search of
`docs/` (specs and audits excluded), `src/cli/` help strings, `.rigger/agents/`, the hidden editor-integration directory,
`skills/`, `docs/handbook/` and `crates/rigger-domain/src/docs.rs` for an id spelled `<x>.rs::`,
`<file>::<name>`, `<name>`, `site:`, `show <id>`, `--show`, a placeholder and a `kind ... degree`
sample: `docs/architecture-addendum-concept-graph.md:23` and
`docs/architecture-addendum-graph-inspector.md:195` (`<file>::<name>` as the id scheme) to criterion
6, naming `<file>::<path>`; `docs/architecture-addendum-graph-inspector.md:200-203`'s resolution query and its expression index to criterion 1, naming the leaf expression and its index, its name-suffix index retirement to criterion 2; `docs/architecture.md:735-736` (a hop auto-continuing through a same-file call) to criterion 3, reading "a same-file call landed on its one candidate"; the `conductor.rs:13035-13039` doc comment split by half, its ownership half (the id format as the sqlite adapter's private convention) to criterion 1, naming the domain's id format and `query::split_id` its one cut, and its never-string-parsed half (each definition's file from the `CONTAINS` edge) to criterion 14, naming `member_file`; its two siblings, `confidence_tier_radius`'s (`conductor.rs:13148-13152`, "never by parsing the `<file>::<name>` id") and `files_reachable`'s (`:13173-13180`, "never an id string-parse"), to criterion 14; the `KIND_CODE_ENTITY` doc (`contextgraph.rs:42-44`, "Its id is `<file>::<name>`") to criterion 6, naming `<file>::<path>`; and the `REL_CONTAINS` doc (`contextgraph.rs:171-172`, the edge from a `file` node to each `code-entity`) and `dash.html:832-833` (`REL_CONTAINS` forward edges file to code entity only) to criterion 9, naming the parent edge. The placeholder
passages (`graph-inspector.md:18`, `:94`, `docs/architecture.md:723`) stay true of `<file>::%<leaf>`,
the `site:` format string (`src/cli/graph.rs:258`, `:263`) is code criterion 4 changes, and no skill,
agent prompt or handbook passage names an id form or a `show` sample; no `around` consumer filters a node on the graph kind `code-entity` (a search of the hidden editor-integration directory's agents, `.rigger/agents`, `.rigger/instructions`, `skills/`, `docs/`, the dash JS and `src`
finds only `dash.html:841`, `:889`, reading the dash's own routes, and `tests/change_path_revert_periphery.rs:187`, reading the CLI `--around` lines, whose kind stays the graph kind, `src/cli/graph.rs:99`, `:141`).

LANDING TREES. At least one assertion per RESOLVER and READER table row a criterion changes (REPLACED, RETIRED, widened or reading a new field) is red on the tree before it, and every assertion is green on its own; a clause worded as no change, 'as at the base' or 'in no ...' is a REGRESSION assertion, green on the tree before as well, as spec 116 exempts its criterion 6's; no earlier criterion asserts the opposite value of the same observable.

| Criterion | Red on (why) | Green on |
|---|---|---|
| 1 leaf | 0, the base (every node-side reader cuts `A::T::new` at the first `::`: `show new` misses it, B's reference stays `ambiguous`, the files lens folds B's placeholder under B; a nameless `B::%new` labels `%new`; a proof recorded in A misses `A::T::new` beside `C::new`; `split_id`, `node_leaf` and `node_leaf_sql()` do not exist; an empty-`name` target is named to the callers walk) | 1 |
| 2 edge side | 1 (the edge-side readers key a seed by `name_suffix` and compare raw text: the callers walk from `A::T::new` misses B's caller, a directly written `B::%new` reference is neither upgraded when `A::new` folds nor demoted when it retires, and `referenced_not_called` misses B; `idx_nodes_name_suffix` exists) | 2 |
| 3 landing | 2 (calls land as at the base on `<file>::<leaf>`: a call `run` beside `T::run` and `S.run` lands on neither, no `%` placeholder is folded; a down hop from a bare `<file>::helper` lands on the project's one `helper`; a call naming a dropped, knowledge-held definition lands on its flat id; a proof names a node no live assertion holds) | 3 |
| 4 renderer | 13 (no attr entries, a file-less node prints `site: <id>:0`, an empty `kind` prints `?`) | 4 |
| 5 extent | 4 (`Def` has no `end_line` and an index without it loads, a read-off definition is unbounded) | 5 |
| 6 ids | 5 (extraction emits flat ids and an index lacking `path` loads; Go `const a, b` siblings and the `var` closure's `T` at the file either way) | 6 |
| 7 bare show | 15 (a path query `Type::method` finds nothing, the nested id existing; `show a.rs::new` and `show a.rs::field` answer none, or a knowledge-held flat id's bare node, beside `a.rs::T::new` and `a.rs::S.field`; `show T::new` answers an artifact `T::new` beside `a.rs::T::new`; `--around new` prints today's note naming no seed form) | 7 |
| 8 caller path | 7 (`enclosing` is a name) | 8 |
| 9 CONTAINS | 14 (the file contains every definition; a `ts-tags-v3` index carrying `path` loads) | 9 |
| 10 one event per path | 16 (twins emit one event each) | 10 |
| 11 rebuild | 10 (`PROJECTION_VERSION` 2) | 11 |
| 12 lanes | - | 12 |
| 13 file half | 3 (`Makefile::all` and an extensionless `file` id bucket by kind; a dotted decision `v1.2-choice` buckets at `(root)`, `file_of` reading its last `.` as an extension) | 13 |
| 14 member rule | 8 (a nested id written directly with no file `CONTAINS` is in no member set or reach) | 14 |
| 15 renamed path | 6 (a definition renamed at the batch head leaves the old leaf's cross-file reference `inferred` and its proof landed) | 15 |
| 16 parent first | 9 (a child emitted before its parent) | 16 |

## Global constraints

- Hyphens, never em or en dashes, ASCII only, in every added line.
- A line citation names the symbol or test at 595cbc59 (main after spec 116, plus the spec drafts), the Design, its tables and TEST DISPOSITIONS alike; a search a sentence dates at 1775b728 is that tree's result; the symbol or test name governs, the line being a pointer only.
- No new event type; no new dependency.
- No index persisted by an interim tree is read by a later one; every test persists in its own temp root (spec 116's constraint).
- Every lane `.rigger/gates/lanes.sh` runs green (fmt, clippy -D warnings, test).
- The planner's batch order is UNIT ORDER's sequence (1, 2, 3, 13, 4, 5, 6, 15, 7, 8, 14, 9, 16, 10 to 12), never checkbox position: 13 precedes 4 and 14, 15 follows 6, 14 precedes 9, 16 follows 9, and 12 lands last.
- No binary of an interim tree is installed.
- Every batch is a pure function of one file's path, bytes and flag; every rebuild deterministic (107).
- The light lane compiles no extraction and no extent (`locate_definition_extent` is
  `#[cfg(feature = "symbols")]`, `src/cli/graph.rs:401`): criterion 5 but its index and nameless-node clauses, the extraction clauses of 6, 8-10 and 16, the fold clauses of 6 and 8
  (`symbols/extract.rs` and `symbols/events.rs`, `#[cfg(feature = "symbols")]`, `symbols/mod.rs:20-28`),
  6's and 7's `--show` clauses assert in the default lane, each such `tests/cli.rs` test carrying `#[cfg(feature = "symbols")]` as `graph_build_records_source_as_ledger_entries_and_folds_it_with_no_run` (`tests/cli.rs:2223`) does; 1-4,
  the index clauses of 6 and 9 (`symbols/store.rs`, ungated), 5's nameless-node clause (`tests/cli.rs`), 7's `--around` note clause (`tests/cli.rs`), 7's, 9's and 15's `contextgraph/sqlite.rs` fold clauses, 14's `subgraph` and `member_sql()` clauses there too, 13 (`contextgraph/query.rs`, `dash.rs`, `console/map.rs`), 14's `contextgraph/query.rs` and conductor clauses, 5's index clause (`symbols/store.rs`) and 11's two clauses (`tests/cli.rs`: the version
  reading 3, no row at the flat nested id carrying `name`) fold, load or render hand-built input in the default and light lanes; the core lane, which compiles neither the grounder, `contextgraph/sqlite.rs`, `dash.rs`, the conductor nor the MCP server (`any(feature = "store", not(feature = "core"))`) and runs no root-crate test, holds only the `contextgraph/query.rs`, `community.rs` and `console/map.rs` clauses of 1, 13 and 14.

## Done when

- [ ] a test proves THE LEAF KEYS RESOLUTION: one cut `query::split_id` and one node leaf `query::node_leaf` with its pinned store form key every node-side reader the RESOLVER TABLE assigns to criterion 1,
  asserted over a nested definition `A::T::new` (and a field `A::S.field`) written directly in file
  A and references from file B folded after it, one per row: in `contextgraph/sqlite.rs`'s tests
  `show new` answering `A::T::new`, `show field` answering `A::S.field` and `show %` a named
  `operator %` node `A::T::%` (`locate`), B's call resolving down onto `A::T::new` (`resolve_down_hop`), B's
  reference `inferred` (`reference_tier`), B's test proof landing on `A::T::new`, a proof recorded in A on its one same-file candidate `A::T::new` beside `C::new`, and two same-file candidates of a proof's leaf in A naming none (`resolve_proofs`), the leaf index serving the pinned leaf query and `same_file_candidates`' read (`migrate_indexes`) and
  `node_leaf_sql()` answering as `node_leaf` over nodes named `A::T::new`, `A.B`, `a::b` and an empty `name`, an empty-`name` node's settle firing no `definition_retired` (`definition_name`), an empty-`name` call target `B::new` counted bare by `callers_via_bare` from a seed `A::new`; in
  `contextgraph/query.rs`'s tests `split_id` over `a.rs::T::new`, `a.rs`, `Makefile::all` and
  `a.rs::`, `file_of` answering as at the base over a design doc, a `#` section and a decision id `foo::bar`,
  `defs_by_entity_suffix` keying `A::T::new` under `new` and the files lens folding B's placeholder under A (`whole_graph_lens_key`); in `dash.rs`'s, `reproject_files` resolving B's placeholder to
  A; in the console map's, `B::%new` labelled `new`, `B::%%x` labelled `%x` and a file node by its
  id; in `community.rs`'s, `Coupling::from_graph` canonicalising B's placeholder onto `A::T::new`.
  This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 1; every edge-side reader is criterion 2's and the live-assertion candidate rule criterion 3's, NOT this one's.
- [ ] a test proves THE EDGE SIDE BINDS THE LEAF: every edge-side reader binds its seed's leaf and matches an edge's target text against the leaf and the sigil-prefixed leaf, and `name_suffix` and `idx_nodes_name_suffix` retire,
  asserted in `contextgraph/sqlite.rs`'s tests over a directly written `A::T::new` and B's
  references to `new`, each spelled `B::new` and `B::%new`: the callers walk from `A::T::new`
  reaching B's caller through both, a directly written `B::%new` reference upgraded to `inferred`
  when a definition `A::new` folds and demoted to `ambiguous` when it retires,
  `referenced_not_called` counting B's file-level reference through both and a reference landed on a
  nested `B::T::new`, a seed with no leaf naming nothing, a C# `operator %` seed reaching its `%%`
  caller, and `name_suffix` and `idx_nodes_name_suffix` gone. This criterion OWNS what ARTIFACT
  OWNERSHIP gives criterion 2; where a call lands is criterion 3's, NOT this one's.
- [ ] a test proves A CALL LANDS ON ITS ONE SAME-FILE CANDIDATE: a call naming one same-file candidate lands on it, else on `<file>::%<leaf>`,
  asserted in `contextgraph/sqlite.rs`'s tests over hand-built events folding each file's
  definitions before its references and directly written nested definitions: a call `run` landing
  on a method `T::run` beside a field `S.run`, a call `Foo` beside a class `N::Foo` and its
  constructor `N::Foo::Foo` on `%Foo`, a call to a provided `Trait::m` beside `<T as Trait>::m` on `%m` and to a required one on the impl at `extracted`, gen A
  defining and calling `foo` then gen B only calling it leaving B's call on `%foo` at `ambiguous`, a top-level `fn
  new` beside a method `new` leaving the call on `<file>::%new`, a type beside a same-named variant
  leaving the call on its placeholder, a dropped definition that a knowledge assertion still holds naming no candidate, its call landing on `<file>::%foo` at `ambiguous`, a call to `x` with no same-file candidate beside a named `<file>::%x` landing on it at `ambiguous`, never `extracted`, a down hop from a hand-built bare `<file>::helper` resolving onto nothing, and a node whose `nodes` row carries `name` with no live assertion carrying it named no proof target, alone or beside a second definition of the leaf (`resolve_proofs`). This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 3; the
  edge-side binding is criterion 2's and the printed fields criterion 4's, NOT this one's.
- [ ] a test proves SHOW IS ONE RENDERER: `rigger graph --show`, the MCP `show` and the MCP `around` print a node's id, file and closed attr list through `site_fields`,
  asserted in `tests/cli.rs` and the MCP server's tests over a const, a field, a finding (its `by`
  and `unit`, and no `file`, no `line`, no `site:`, no body and no note on any surface), an
  architecture decision (its `title` and `doc`, its markdown path as `file`), a `docs/x.md#sec`
  section (`docs/x.md` as `file`) and a code entity `Makefile::all` (`Makefile` as `file`) by exact
  id, the `site:` header line unchanged for a code node, the MCP object and the CLI's entry lines, after the CLI's two print transforms, each equal to
  `site_fields`' output, the CLI's entry lines compared on the ordered `site_fields` entry and value pairs and the MCP object on those pairs without order, its `line` and `degree` JSON numbers, a `doc` holding a newline printed on one CLI line with spaces and carried verbatim
  in its MCP key, an attr outside the list not printed, a finding's `summary` printed,
  an empty field printing no line and no key, a hand-built `kind: ""` printing its graph kind, and every `around` node carrying `entity_site`'s fields over its loaded `Node`, its degree its edges in the merged `Graph` `around` returns, with no per-node `site_of` or `one_hop_degree` read, a node at the depth bound showing a lower degree than `show` gives it.
  This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 4; the body bound is criterion 5's, NOT this one's.
- [ ] a test proves ONE EXTENT BOUNDS A BODY: every `Def` carries `end_line` from `extract::def_of`, and `extract::locate_extent` bounds a body by recorded name, kind and line, else recorded name and line, else a drifted line by name and kind,
  asserted in `symbols/extract.rs`'s tests over one fixture per registered language, each holding a
  tagged and a read-off definition, an attributed or decorated one's `line` still its name row, an ambiguous name left unbounded, two tier-1 matches bounded by the wider, a drifted production fn beside a `#[cfg(test)]` twin bounded to the production one, a definition whose kind changed at its recorded name and line, beside a same-named definition of the recorded kind elsewhere in the file, bounded by the second tier at its recorded line, a zero-length range given `end_line` = `line`, a drifted fn beside a
  same-named field healed and a variant inside its same-named enum bounded to the variant, and in
  `tests/cli.rs` with `rigger graph --show` of a `.tsx` definition bounded under its TSX grammar and
  `definition_extents` and `DefExtent` gone, and, ungated, a hand-built nameless node at a non-zero line printing that note with its id; and in `symbols/store.rs`'s tests a stored index lacking
  `end_line` loading as absent. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 5; nested ids are criterion 6's, NOT this one's.
- [ ] a test proves IDS FOLLOW THE LANGUAGE'S PATH: extraction gives each definition its `<file>::<path>` id,
  asserted in `symbols/events.rs`'s tests through the fold over two `new` methods of two types, a
  trait impl method, an impl nested in an inline mod, a variant, a field, a tuple field, an
  associated const, a fn-local item, a Go receiver method, a Go `const a, b = f(), g()` as two
  siblings under the file, a TS class field `C.f`, a C# constructor `N::Foo::Foo`, a C#
  `namespace N; class Foo {}` at `N::Foo`, an `impl Foo<u8>` method at `Foo<u8>::new`, `const f = function f() {}` and `const Foo = class Foo {}`
  at `f` and `Foo`, two `const _: () = { fn a() {} }` blocks in one file at `a`, a Go `var a, b = func() { type T struct{} }(), 1` at `T`, and a Python def nested in a function `outer::inner`, each at its IDS FOLLOW THE LANGUAGE'S PATH id, and an event without `path` and one whose `path` is empty each folding `<file>::<name>`; in `symbols/store.rs`'s tests a persisted index lacking `path` loading as absent; and in
  `tests/cli.rs` `rigger graph --show` of a drifted `T::new` beside `U::new` bounded to `T::new`'s
  body. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 6; bare `show` by path is criterion 7's and a renamed path's retirement criterion 15's, NOT this one's.
- [ ] a test proves A BARE SHOW FINDS A LEAF OR PATH: after the exact id and the leaf, `show` answers the leaf lookup filtered on the id, by the suffix `::<query>`, compared byte for byte, then by the query's `<file>::` range,
  asserted in `contextgraph/sqlite.rs`'s tests over hand-built events carrying criterion 6's
  `path`: `show Type::method`, `show Struct.field` and `show Kind::Constant`, `show A.B` answering a
  `namespace A.B` in one file before a field `B` of a struct `A` in another, and in `tests/cli.rs`
  with `rigger graph build` over a real Rust file then `rigger graph --show Struct.field`; and in `contextgraph/sqlite.rs`'s tests over hand-built events `show a.rs::new` showing
  `a.rs::T::new`, `show a.rs::field` the field `a.rs::S.field`, `show a.rs::new` still `a.rs::T::new` while a decision holds the flat `a.rs::new`, `show T::new` answering `a.rs::T::new` beside an artifact `T::new`, and listing `a.rs::T::new` and `a.rs::U::new` when both exist; and in `tests/cli.rs`, ungated, `rigger graph --around new` printing the not-found note that names the exact-id seed form and `--show`. This
  criterion OWNS what ARTIFACT OWNERSHIP gives criterion 7; the caller's path is criterion 8's, NOT this one's.
- [ ] a test proves A CALLER IS NAMED BY ITS PATH: `SymRef.enclosing` and `EdgeInferred.caller` carry the caller's path,
  asserted in `symbols/extract.rs`'s and `symbols/events.rs`'s tests: a call in `impl T { fn new() }`
  folding `CALLS` from `<file>::T::new`, one in a Go receiver method from `<file>::Recv::name`, and a
  hand-built leaf caller folding from `<file>::<leaf>`. This criterion OWNS what ARTIFACT OWNERSHIP
  gives criterion 8; the parent edge is criterion 9's, NOT this one's.
- [ ] a test proves CONTAINS IS STRUCTURE: each definition's `CONTAINS` comes from its parent, the file holding only top-level definitions,
  asserted in `symbols/events.rs`'s tests through the fold and in `contextgraph/sqlite.rs`'s over
  hand-built events, a variant folded before its enum, a Go method on a receiver the file does not define contained by the file and one on a receiver it
  defines contained by that type, `namespace N; class Foo {}` with `Foo` contained by `N`, `impl Display for T` in a file not defining `T` containing its
  method under the impl, a removed variant's `CONTAINS` retired at the next generation, `var a, b = func() { type T struct{} }(), 1` giving `T` the file as parent beside `a` and `b`, and a
  hand-built event naming a parent no event defines leaving that parent a nameless node no
  `CONTAINS` from the file reaches, and a hand-built event whose `parent` is empty and one whose `parent` names its own path each contained by the file; and in `symbols/store.rs`'s tests a persisted `ts-tags-v3` index carrying `path`
  loading as absent. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 9; the emission order is criterion 16's, twins criterion 10's and the member rule criterion 14's, NOT this one's.
- [ ] a test proves ONE PATH IS ONE EVENT: `extract_events` emits one event per path, its winner the first by spec 116's `CODE_KIND_PRECEDENCE`, then the lower line,
  asserted in `symbols/events.rs`'s tests over `mod x;` beside `fn x`, two cfg twins and two impls
  with one header, two same-path, same-precedence twins on one line keeping the first `extract`
  returns, a `#[cfg(test)]` twin never winning, a removed twin leaving the other's line and parent, `const f = function f() {}` emitting one event at `f`, and a TS `namespace A.B` beside `class A { B }` emitting one event at `A.B`, the field winning. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 10; the parent edge is criterion 9's, the emission order criterion 16's and a renamed path's retirement criterion 15's, NOT this one's.
- [ ] a test proves AN OLDER GRAPH IS REBUILT: a `graph.db` folded at `PROJECTION_VERSION` 2 owes its rebuild, after which its stored version reads 3 and no row at a flat nested-definition id carries `name`,
  asserted in `tests/cli.rs`. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 11; the ids are criterion 6's and the index version criterion 9's, NOT this one's.
- [ ] every lane `.rigger/gates/lanes.sh` runs green (fmt, clippy, test). This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 12, no artifact: only the lanes over the integrated result.
- [ ] a test proves THE FILE HALF IS ONE READ: `query::node_file(kind, id)` gives the file an id names by its kind table, and every `file_of` reader reads it,
  asserted in `contextgraph/query.rs`'s tests: `cluster_key` folding `Makefile::all` to `(root)`, a
  design doc to its directory, a decision `v1.2-choice` to its kind and a `file` id with no
  extension (`src/utils/Makefile`) to its directory, and the files lens and `card` giving it `file` `Makefile`; in `dash.rs`'s tests `reproject_files` folding `Makefile::all`
  under `Makefile`; in the console map's, `Makefile::all` counted in `Makefile`'s district. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 13; the printed `file` is
  criterion 4's and the member rule criterion 14's, NOT this one's.
- [ ] a test proves THE MEMBER RULE: a file's members are its named code entities by `query::member_file`, and every REPLACED `CONTAINS` reader reads it,
  asserted over nested ids with no file `CONTAINS` (UNIT ORDER: in-memory `Graph`s, direct writes in `contextgraph/sqlite.rs`): in `contextgraph/query.rs`'s and the conductor's tests a file's `member_set` holding its nested method and `Makefile::all` in `Makefile`'s, a placeholder `<file>::%new` and a bare parent `<file>::P` beside it in no member set, nor an `artifact` `a.rs::x`, and `files_reachable`, `write_code_neighborhood` and `neighborhood` giving the nested method its file and the placeholder none; in `contextgraph/sqlite.rs`'s `subgraph`, seeded at the file holding the seed alone at depth 0 and the nested method at depth 1, and seeded at the nested method its file at depth 1, the bare parent in its range in neither, a named in-range node only an unkeyed assertion holds loaded as a member at depth 1, and there `member_sql()` and `member_file` agreeing over a file's member, a placeholder, a bare parent and a knowledge-held flat id. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 14; the `CONTAINS` writer is criterion 9's, NOT this one's.
- [ ] a test proves A RENAMED PATH RETIRES ITS OLD LEAF: a definition refolded at its path under a different leaf retires the old leaf's convergences, as a cold rebuild folds them,
  asserted in `contextgraph/sqlite.rs`'s tests over hand-built events: `B` at a call-target kind refolded as `A.B` on one node at the batch head leaving the old leaf's cross-file reference `ambiguous` and its proof unlanded, and a hand-built differing-name pair in one batch taking the same outcome. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 15; the path ids are criterion 6's and the dedup criterion 10's, NOT this one's.
- [ ] a test proves A PARENT IS EMITTED BEFORE ITS CHILD: `extract_events` emits each definition after every definition that contains it,
  asserted in `symbols/events.rs`'s tests: a variant that sorts before its enum emitted after it, and a nested method after its impl and its type. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 16; the parent edge is criterion 9's and the dedup criterion 10's, NOT this one's.
