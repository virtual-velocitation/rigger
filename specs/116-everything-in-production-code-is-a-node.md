# 116 - Everything in production code is a node

**Goal:** the knowledge graph is the lookup surface every persona uses; a question it cannot answer
becomes a grep. Today it is blind to most of what production code defines. The Rust tags query
(`RUST_TAGS`, `crates/rigger-grounder/src/grounder/symbols/registry.rs:19-40`) has no pattern for a
`const`, `static`, enum variant, field, trait required fn, associated type or `extern crate`, so
none of the 153 live `constant` nodes is Rust and `show TYPE_CODE_ENTITY_EXTRACTED`,
`show DERIVED_INDEX_TYPES`, `show RUST_TAGS` and `show LOCKS` answer `{"status":"none"}`. Measured
on the production Rust of `crates/` and `src/`, that is about 2,000 definitions beside today's
4,550 (+45%). Go has no const or var, C# no struct, enum, record, field, property or constructor,
TypeScript no enum, type alias or `as const` constant, Python no class attribute, and a call inside a function-local
data definition is attributed to that definition instead of its function. This spec makes every
definition production code holds a node of its kind, in every registered language, with the
function that encloses a reference as its caller.

## Design

**PRODUCTION CODE IS THE BOUNDARY, decided here.** "Everything in the code" is production code:
the source of the package's shipped targets (Cargo lib and bin target sources and `build.rs`);
dev-only code, the items the existing attribute walk marks test (`#[test]`, or a `cfg` predicate
gating on `test`; `extract.rs:517-531`), the files a DEV-ONLY PATHS pattern (Notes)
names and the out-of-line test module set below, produces no definition node and no structural
edge. ONE predicate, `events::is_dev_only(path)`, replaces `is_under_tests_dir` (`symbols/events.rs:535-539`)
at all four of its uses: `extract_events`, `proof_events`, the design pass's `doc_links` and
`scored_hits` (`symbols/grounder.rs:428`), so ground ranking and the graph read one predicate (criterion 1's), a path a pattern names being
demoted in ranking as a consequence of the swap; the graph's boundary reads a second input as today,
the out-of-line test module set (`out_of_line_test_module_files(idx)`, `symbols/events.rs:737`),
which ranking does not read (OUT OF SCOPE). A file a DEV-ONLY PATHS pattern names and the out-of-line set does not has its references emitted
as proof evidence as a `tests/` file's are today (spec 86), a `benches/` or `examples/` file's joining `proofs` from criterion 1, accepted: they exercise production code, and a proof
landing across languages (`shim/shim.test.mjs` on a uniquely named Rust definition) is today's
`resolve_proofs`, accepted; an out-of-line `#[cfg(test)]` module target emits none, as today (`symbols/events.rs:410-420`), kept, and a file
both inputs name (a `src/foo/tests/mod.rs` that `#[cfg(test)] mod tests;` declares) takes the
out-of-line set's rule, emitting no proof as today (`lower_file`, `symbols/events.rs:162-164`). A dev-only
script outside the patterns is production, a production module under `benches` or `examples` makes no node, a C# product directory whose name ends in `Test` or `Tests` makes no node, and `doc_links` drops a design link to a path `is_dev_only` names, gated patterns and markdown
paths included, as it drops a `tests/` path today; all accepted. A `#[cfg(test)]` definition of any kind is a test definition (`Def.is_test`,
which `extract_events` excludes): the attribute walk (`preceded_by_test_attribute`,
`symbols/extract.rs:270-288`) starts from the outermost node below the root whose range equals the
definition's, since `descendant_for_byte_range` answers a unit variant's name child whose range
equals the variant's; a read-off definition passes the node it is read from, and a tuple field,
which the grammar gives no member node (`ordered_field_declaration_list` holds `attribute_item*`, an
optional `visibility_modifier`, then the `type` field), passes its type node, its walk first stepping
over a preceding `visibility_modifier`, since `scan_attribute_stack` (`extract.rs:399-420`) stops at
any other node. So a `#[cfg(test)]` field, variant, const, static or tuple field, `pub` or not, is
skipped by the one walk every item already uses (criterion 6's; the attribute is Rust's alone), a
new kind's tagged range feeding `test_regions` (`extract.rs:210-220`) as every definition's range, tagged or read-off, does today; the walk change is part of the Rust pattern mechanism, one owner with it, since it exists so
every new kind's `is_test` holds; it includes ONE inner-only count, `leading_inner_test_attribute`'s (`extract.rs:382-387`) at a definition's body, which counts `inner_attribute_item` nodes only, never `scan_attribute_stack`'s outer `attribute_item` arm (`extract.rs:408-411`, which the backward sibling walk, `:287`, and `out_of_line_path_override`, `:445`, keep), so `struct S { #[cfg(test)] probe: u32, real: u32 }`, `enum E { #[cfg(test)] A, B }` and `impl X { #[cfg(test)] fn t() {} fn real() {} }` keep `S`, `real`, `E`, `B` and the impl production, the one walk skipping only the attributed member; the owner kinds the count flips (fn, mod, impl, trait, struct, enum, union) are criterion 6's to hold, its existing-kind cases (that impl, `fn f() { #[cfg(test)] let x = 1; }`, `mod outer { #[cfg(test)] mod inner {} }`) 6's red assertions for the existing owner kinds, their flip being the inner-only count's observable, not a guard, excluded from 11; the count is one mechanism whose existing-kind flip (an owner and all it holds turning production) is an accepted consequence, the nodes it adds arriving by the generation rule at the next ingest; the count is required by criterion 6's member kinds, a struct or enum body's first member attribute otherwise marking the owner and every member test, so the existing-kind flip is the same mitigation's range, not a second one; the root read below (criterion 11's) reuses that count and is 11's to assert, a file opening `#[cfg(test)] use x;` staying production. A TEST REGION IS EVERY TEST-ATTRIBUTED RANGE, a mechanism apart
from the Rust patterns: beside a test-attributed definition's range, tagged or read-off, `test_regions` reads a
test-attributed definition the name filter drops, the filter removing it from `defs` and from the
caller rule's ranges only; a test-attributed `foreign_mod_item`'s range, an extern block being a
container that yields no `Def`; and the whole file, when the root's leading inner attribute stack
names `test` (`#![cfg(test)]`, read forward from the root's first named child as `leading_inner_test_attribute` reads from a body's). So the inner items of a `#[cfg(test)]
const _: () = { .. }`, a `#[cfg(test)] extern "C" { .. }` and a file opening `#![cfg(test)]` are
test code, every existing kind after that opening included, which today marks only the item after it
(`extract.rs:285-288`), and a `mod x;` in such a file joins the out-of-line test module set; each
source turns definitions and references test whatever the Rust patterns tag, so it is criterion 11's row.

**SCOPE, decided here.** A NAMED node type is one whose `fields` in the grammar's `node-types.json`
hold `name`; that file's `named` flag is not meant. Every definition the grammar can name in a production file becomes a node
with its kind, in every registered language (Rust, Go, C#, JS, TS, Python): DEFINITION TYPES and
KINDS (Notes). `Kind` (`symbols/model.rs:27-36`, a `pub` enum) gains `Constructor`, `Macro` and `Declaration` at criterion 2, the caller rule's vocabulary, and each other new variant with its `kind_str` (`symbols/events.rs:775-786`) and `kind_of` arms (`symbols/extract.rs:13-24`) at its first producer: `Static`, `Variant`, `Field` at 6,
`Variable` at 7, `Property` at 8; the `macro` `kind_of` arm lands at 6 and the `constructor` one at
8. A Rust trait lowers to `trait`, `macro_rules!` and a `#[proc_macro*]` fn to `macro`, and `Other`
stays only for an unknown tag category. Every read-off is a named function `extract` calls beside
`impl_blocks`, dispatched on its existing `lang` argument, `LanguageEntry` and `extract`'s
signature unchanged; a rule re-kinding a name upstream already tags (Python's method rule, every re-kinding) is such a read-off over the parse tree, as `is_declaration` is, never a second pattern on that name node (`composed_once` appends after upstream, `registry.rs:130-160`). Items under a non-test `#[cfg(..)]` stay product code. A local
variable is no definition in any language (Go's function-local `var`, a C# method-local
`variable_declarator`, a Rust `let`, a Python local assignment), while a JS or TS
`variable_declarator` at any depth stays one (`registry.rs:91-125`) and takes its value's kind
(criterion 9's): `function` for a function expression, arrow function or generator function value,
async ones included; `type` for a class value, anonymous or named, the declarator's name being the
class's (a named class expression also keeps its own definition, as upstream `tags.scm:15-16` tags
it today); `constant` for every other value, a parenthesized or type-asserted function among them,
and for a valueless declarator. The upstream query tags only arrow and function-expression values
as functions (`tags.scm:44-61`), and the constant list is a query enumeration with no Rust
post-filter beside it (`JS_CONSTANT_TAGS`, `registry.rs:98-103`, `:115-120`), so criterion 9 adds a
`generator_function` value to the function patterns, a `class` value as `type`, every remaining
expression type of each grammar that is no function expression, arrow function, generator function
or class to the constant enumeration (one composed query per grammar, JS, TS and TSX, `.tsx` taking its own in `for_extension`, each
enumerating its own grammar's types: TS and TSX add `as_expression`, `satisfies_expression`,
`non_null_expression`, `instantiation_expression` and `internal_module`, an `internal_module` value
being a constant, TS alone `type_assertion`, JS and TSX `jsx_element` and
`jsx_self_closing_element`), which a test checks against each grammar's `expression` supertype read from
`NODE_TYPES`, `TYPESCRIPT_NODE_TYPES` and `TSX_NODE_TYPES`, flattened through `primary_expression`:
every subtype is in the enumeration but `function_expression`, `arrow_function`,
`generator_function` and `class`, so a missing type is a red test, a valueless declarator by the negated field `!value`, and the
destructuring bindings THE LEAF decides. Any test-attributed node that is not one of criterion 11's listed sources (a statement, expression,
macro invocation, match arm, field initializer or parameter) yields no `Def` and no test region, its references staying production, accepted. A JS or TS
`constructor` method is kind `constructor`, and a JS or TS class field whose value is an arrow
function or function expression is kind `method`, as a `variable_declarator` takes its kind from its
value today. A Python `function_definition` is a method exactly when its nearest enclosing definition is a `class_definition`, a `decorated_definition` wrapper
included, else a function (the upstream query tags every one a function, `tree-sitter-python`
`queries/tags.scm`). New definitions flow into every grounder consumer of `fs.defs` (`name_occurrence_map`, `scored_hits`, `ground_ranked`'s definition sites,
`blast_radius`'s definers), unchanged code: `ambiguity_map` and `commonness_map`
(`grounder.rs:291-335`) count the new leaves, so `has_strong_match`'s percentile cutoff and ranking
shift, and `blast_radius` (`:627`) counts a file defining a same-named field a definer, accepted: a
name a field or variant shares is that much more ambiguous to a reader, and the one new kind in
`grounder.rs`'s fixtures, `store_tree`'s trait required fn (`:1507`), matches its tests' queries by
path alone, so their pins stand (TEST DISPOSITIONS). The safe view of a name the index defines is its definer files plus the files
the index's reference patterns reach by its leaf, with no text search: main's decision at PR #73 (`blast_radius`, `grounder.rs:605-607`, `:624-635`, `:687-691`), pinned for
a type, the struct `Store`, by `blast_radius_text_searches_only_the_spans_the_index_does_not_resolve`, and holding for a Rust type today since the Rust patterns hold call references only (`registry.rs:19-40`).
This spec's kinds join that contract, accepted by kind and language with no closer: Rust's constant, static, variant, field, associated type, `extern crate` module and declaration (6); Go's constant, package-level variable, field, `type_alias` type and declaration (7); C#'s struct, record, enum and delegate types, variant, field, constant, property, namespace module and declaration (8); JS's and TS's constant, field, class-valued type, enum and type alias types, module, variant and declaration (9); and Python's field, constant and type alias (10) each ground on their definer files plus the files whose references a pattern of their language captures by the leaf (a call in every language; C#'s `@reference.class`, `@reference.interface` and `@reference.send` patterns (`registry.rs:49-70`), TS's `type_annotation` and Go's `type_identifier` type references, a JS or TS `new_expression`), every other use (a read of a constant, static, variable, field, property or variant, a Rust type mention, a member access that is no call) reaching no file, so a kind none of whose uses a pattern captures grounds on its definer files alone.
The cost, once: a semantic conflict between co-scheduled units (one changing such a definition, another adding a reader of it in another file) merges textually clean and is caught by the integrated tree's gates at the landing, charging that landing one attempt; the gain: a member name with many readers (`id`, `name`) is text-searched across the tree today, so a unit touching it is co-scheduled with nothing, and from 6 it grounds on its definers. `definition_extents`
(`extract.rs:716-756`) runs the same tags query unchanged: a tagged new definition gains its extent,
a read-off one has none and `show` prints its site with no body (`locate_definition_extent`'s miss note, `src/cli/graph.rs:440-443`), as for an impl block today, a definition whose
leaf differs from its tag's name slice (`extract.rs:736`: a C# destructor, a string-named member) has none under its leaf, its extent being emitted under the raw slice (`extract.rs:749-750`), so `show` of it prints that note until spec 119's `def_of`, and from 8 a C# type sharing its name slice with its constructor or destructor loses the one-candidate heal (`locate_definition_extent`'s tier 2, `src/cli/graph.rs:429-440`) until spec 119's `def_of`, accepted by name, and a leaf several
definitions share is bounded by its rule today, accepted; a read-off or binding-walk winner whose leaf a tagged definition shares in its file prints the tagged definition's body (`locate_definition_extent`'s one-candidate heal, `src/cli/graph.rs:429-440`) until spec 119's `def_of`, accepted by name. Each language's
DEFINITION TYPES list (Notes) is its criterion's (6-10), which 6-10 read, a listed type without a
pattern being its language criterion's defect; criterion 12's gate owns only the classification of
`NODE_TYPES` and adds no pattern: every node type carrying a `name` field sits in exactly one list,
checked in that direction only, a read-off entry's type exempt when it carries no `name` field and checked like any listed type when it does (Go's `type_alias`), a read-off row's type column holding the member's node type, never its container, save the Rust tuple-field row, which holds `ordered_field_declaration_list`, the grammar giving a tuple field no member node, and a type found in both lists being its language criterion's defect; a type it finds absent that THE RULE classifies a definition is a SPEC DEFECT, closed by amending that language's list and query (6-10) before launch, and found during the run is UNIT ORDER's escalation, naming its language criterion; `<LANG>_NON_DEFINITIONS` is criterion 12's, an absent type THE RULE classifies a non-definition being 12's to add.

**THE LEAF, decided here.** A definition's leaf (`Def.name`) is its name node's text, and where that
is no bare identifier: a dotted or qualified name (C# `namespace A.B.C`, file-scoped too, TS
`namespace A.B`) is its whole text as written; a string name (TS `declare module "x"`, a JS or TS
member `"a-b"`) is the raw source text between the quotes, escapes undecoded, two spellings of one value being two leaves; a numeric member name is its text; a JS or TS
private member keeps its `#`; a C# destructor is `~` and its identifier, a tagged definition whose leaf is not its name slice (`extract.rs:56` takes `tag.name_range` verbatim: the destructor, a quoted member or module) being renamed by ONE named leaf function in `extract`, `extract::def_leaf`, landing at 8 with C#'s arm and gaining JS and TS's at 9, which runs before the definition enters `def_ranges` (`extract.rs:64`, where one `name` feeds both today), so the caller name is always the leaf; a C# operator is its
operator token, an indexer `this`, and a conversion operator its header from `implicit` or
`explicit` through the target type, whitespace-collapsed (`implicit operator int`); a Rust tuple
field is its index; an `extern crate` is its `as` alias when present, else its name; a Go embedded
field is its type's base identifier, pointer, package qualifier and type arguments stripped, the name Go gives it; a
Python `type_alias_statement`'s leaf is the identifier its `left` type holds, directly or as a
`generic_type`'s identifier child (`type X[T] = ...` gives `X`). For a destructuring binding the leaf is each bound identifier's text: ONE binding walk (criterion 9's) over a per-grammar table of pattern node kinds (which bind,
rename, default, rest, nest or bind nothing) makes every bare identifier a pattern binds one
definition, in JS and TS for a `variable_declarator` whose name is an `object_pattern` or `array_pattern` and for a `for_in_statement` whose `kind` is `const`, `let` or `var`, at any depth, whose `left`
binds as a declarator's name does: `node-types.json` gives `left` the types `identifier`,
`object_pattern`, `array_pattern`, `member_expression`, `subscript_expression`,
`parenthesized_expression` and `undefined`, and in TS and TSX `non_null_expression`; an
`identifier` (`for (const k of m)`, `for (var i in o)`) is one definition whose extent is its own range, so the loop body's references take the next
enclosing definition, produced by the binding walk, never a tag, so with no `definition_extents`
extent, the two patterns are walked by this rule, and every other type binds nothing; a `for_in_statement` with no `kind`
assigns existing names and binds nothing, whatever its `left`. The walk's seeds (which `left` nodes enter it, a chain followed down `right` while it is an `assignment`, a bare far-right identifier never binding) are each language's read-off function's, the table holding pattern kinds only; criterion 10 adds Python's rows and seeds, covering a module-level or class-body assignment whose target is a `pattern_list`, `tuple_pattern` or
`list_pattern`, and each target of a chained assignment (`x = y = f()`), module-level and
class-body meaning a direct child statement of the module or the class body, as the upstream query
matches today, so an assignment under `if TYPE_CHECKING:` or `try:` is none, accepted as today. A rename `{a: b}` binds `b`, a default `{a = 1}` or `[x = 1]` binds `a`
or `x`, a rest `...r` or `*r` binds `r`, a nested pattern is walked down, a computed key's value
pattern binds as any other, and an array hole, an empty pattern, a `member_expression`, an
`attribute` and a `subscript` bind nothing. Each bound identifier is kind `constant` in JS and TS
and takes its target's kind in Python (constant at module level, field in a class body, one with no `right`, `x: int`, a field as one with a value is), with the identifier's own range as its extent, so the initializer's references take the next
enclosing definition by the caller rule (a module-level `x = y = f()` gives `f` the file as caller);
the Python query keeps the upstream module-level pattern only where `right` is absent (a `!right` pattern, `x: int`) or one of `node-types.json`'s admitted types but `assignment`, `right: [(augmented_assignment) (expression) (expression_list) (pattern_list) (yield)]`,
so a single-target constant stays a tag with today's extent, the whole assignment, and a chained
assignment's targets (the first included) come from the binding walk alone and have no
`definition_extents` extent. A JS or TS member whose name is a `computed_property_name`, and a
Python assignment target that is an attribute or a subscript (`self.x = 1`, `d[k] = 1`), is no
definition, nothing naming it statically; a Python `for` target is no assignment and binds
nothing, accepted. ONE name filter in `extract`, taking the file's language, drops every `Def` whose leaf is the empty string, and in Go and Rust,
where the grammar makes `_` the blank identifier, every `Def` whose leaf is `_` (criterion 3's); a
JS, TS, Python or C# `_` is a named definition its `_(..)` calls resolve to and stays one; a
filtered definition's body's references take the next enclosing definition by the caller rule. Today's queries capture none of the non-identifier shapes above
(`registry.rs:70` takes a C# namespace's `identifier` only, the TS `module` and JS
`method_definition` patterns an identifier only), so none of them changes an existing leaf; the
filter does drop every Go definition named `_` extracted today (function, method, type; the upstream Go query),
which retires by the generation rule (`retire_unheld_nodes`, the DROPPED row), while a JS
`const _ = require(..)` (`registry.rs:95-104`) and a Python module-level `_ = gettext.gettext` (the upstream Python query) keep their nodes.

**A DATA MEMBER IS NO CALL TARGET, decided here.** Every site that decides whether a node is a
definition reads the `name` attr today. `rigger-domain` exports ONE non-target kind list, `contextgraph::NON_TARGET_KINDS` (the `field`, `property` and `declaration` constants), beside the
code kind constants, and ONE function, `contextgraph::is_call_target(kind: Option<&str>)`, false
exactly for a kind in that list; `rigger-graph-sqlite` builds its own SQL predicate,
`call_target_sql()`, from that list beside the readers that use it, coalescing an absent `kind` to
the empty string before the comparison, so a named node with no `kind` attr is a call target in
both forms. Criterion 4's agreement test enumerates the kind constants in a test-local array, its crate reaching no
`kind_str`, and criterion 5 re-points it at `CODE_KIND_PRECEDENCE`, the one production list of them. The
domain owns which kinds are no call target; the adapter owns how its store asks. A
node is a CALL TARGET when it carries `name` and its `kind` attr passes them (criterion 4's). The table below IS the reader list, every site in the tree deciding
definition-ness by the `name` attr given one disposition: CALL-SIDE (a walk, tier, proof,
retirement or candidate read) takes the call-target test, so a field or property node is to it what
a bare placeholder is, `callers_of` excepted; LOOKUP (show, locate, labels, the files lens's own-file test) keeps every
node carrying `name`. Every reader is in this table, from a walk of the production code of
`src` and every `crates/*/src` for `contains_key("name")`, `node_has_name`, `attrs.get("name")`, `$.name` and `"name"`.

| Site | Disposition |
|---|---|
| `node_has_name` in `resolve_down_hop` and `callers_of` (`contextgraph/sqlite.rs`) | CALL-SIDE, through the adapter's `call_target_sql()` as every SQL row is; `callers_of` returns none BEFORE `callers_direct` (`sqlite.rs:3416`) when `cur`, the seed or a node reached at any depth, carries `name` and fails `is_call_target`, never ascending through its leaf (`:3419`), whose callers are same-file calls naming the leaf, an attr-less placeholder keeping its direct callers as today |
| `definitions_with_suffix` (`sqlite.rs`), given a `call_side` argument | CALL-SIDE for `resolve_down_hop` and `callers_of`; LOOKUP for `Projector::locate`, so `show <field leaf>` answers the field, one of `locate`'s candidates (`Located::Many`) when another file's definition shares its leaf, criterion 4's `locate` clause asserting membership in that list |
| `callers_via_bare`'s bare test | CALL-SIDE |
| `reference_tier`, both arms | CALL-SIDE |
| the definition arm's tier upgrade and its `resolve_proofs` call | CALL-SIDE: they run only when the settled node, its `kind` attr after `ensure_node`, is a call target, never by the event's kind |
| `definition_retired`'s remaining-definition test | CALL-SIDE |
| `resolve_proofs`'s definition set | CALL-SIDE |
| `definition_name`, `settle_node`'s retirement trigger | CALL-SIDE |
| `query::defs_by_entity_suffix` (`rigger-domain` `contextgraph/query.rs`), the files lens's and `reproject_files`'s candidate index | CALL-SIDE |
| the files lens's own-file test (`query.rs`, `Lens::Files`), `reproject_files`'s bare test (`rigger-dash` `dash.rs:1830`) | LOOKUP: a named node folds under its own file |
| `write_code_neighborhood`'s definition list (`rigger-conductor` `conductor.rs:13086`) | CALL-SIDE: no field, property or declaration takes a slot of its capped list (`conductor.rs:13082-13121`) |
| `console::map::build`'s drawn-node test (`rigger-console` `console/map.rs:244`) | LOOKUP: a named node is drawn |
| `Coupling::from_graph`'s redirect target, `defs_by_name` (`rigger-domain` `community.rs:130-134`) | CALL-SIDE: a placeholder never redirects onto a field |
| `Coupling::from_graph`'s `bare` test, the redirect subject (`community.rs:130-134`) | LOOKUP: a node carrying `name` is never redirected, so a field keeps its own edges and community |
| `query::node_label`, `concepts.rs`'s `node_label`, `console::map::build`'s label (`console/map.rs:331-335`), the community relabel's `COALESCE(name, id)` (`sqlite.rs`) | LOOKUP: a label |

Not readers: the entity arm's `name` write, a test payload in `contextgraph/mod.rs`, and every `name` that is no graph node attr (TOML, JSON and config keys, tree-sitter field reads). So a same-file call `x.kind()` in a file defining a field
`kind` resolves down by leaf as today, a field sharing a fn's leaf adds no candidate to its
frontier, no proof moves onto it, and `show kind` still answers the field among `locate`'s candidates. `callers_of` at a declaration seed answers none, its callers being reached from the implementation through `callers_via_bare`, accepted by name; a declaration no implementation in the tree answers (an `extern` fn, a `declare function`, an abstract or `extern` C# member, a bodiless Go func) keeps its direct callers as the `CALLS` edges `callers_direct` reads (`sqlite.rs:3416`), while `callers_of` answers none there as criterion 4 states, accepted. A variant, constant,
static, variable or constructor stays a call target, each callable (a tuple variant, a function
value, a C# constructor by its type's name), a shared leaf making a frontier and, shared across files, holding a cross-file proof back
(`resolve_proofs` lands a cross-file proof only on a name's one definition, `sqlite.rs:3657-3660`, a same-file one taking precedence, `:3679-3681`), accepted by name, and a callable field or property (a
function-typed TS `property_signature`, a Go func-typed field, a C# delegate field) is no call
target, accepted by name: its node exists, spec 119 keeping this call-target test unchanged (its call row reads `NON_TARGET_KINDS`); a destructured import binding (`const {a} = require("x")`) is a `constant` and a call target, so a same-file `a()` lands on it and is no caller of another file's `a`, accepted as a plain `const x = require(..)` behaves today (`registry.rs:95-104`); a
pre-spec store holds no field, property or declaration node, so every existing assertion stands. A DECLARATION,
kind `declaration`, is a function, method or constructor with no body: a node of a signature type,
or of a type whose grammar's `body` field is optional holding no `body` child (DEFINITION TYPES'
declaration entries; Python and JS have none), as ONE function `extract::is_declaration(node)` reads
over the definition's own tagged or read-off node, the outermost equal-range node being the attribute walk's start only, consulted only for a definition whose kind is function,
method or constructor; each signature-type pattern tags `@definition.function` or `@definition.method`, `declaration` being reached only through `is_declaration`, never a `kind_of` arm. It is no call target, so a port trait or interface
whose one implementation sits in another file keeps one candidate, its calls resolving down onto it
and its proofs landing as today; and a TS signature or a bodiless C# or Go member, a `function` or
`method` today, re-kinds at its language's criterion, all accepted. At flat ids a
field and a fn of one leaf in ONE file share a node, which is criterion 5's alone; criterion 4's
field and fn of one leaf sit in different files. Every code kind's string is ONE constant in
`rigger-domain`'s `contextgraph`, which `kind_str` and this predicate both read: each lands with its first production reader, `constructor`, `macro` and `declaration` at 2 with their `kind_str` arms,
`static`, `variant` and `variable` at 5 with `CODE_KIND_PRECEDENCE`, and every other at 4, which `kind_str` and `NON_TARGET_KINDS` read.

**A SHARED NODE TAKES ONE KIND, decided here.** At this spec's flat ids every definition of one leaf
in one file folds onto `code_entity_id(file, leaf)` (`sqlite.rs:3170`), whose `kind` and `line`
attrs `ensure_node` (`:3748`) and `record_assertion` (`:3901`) merge by `json_patch`, the last
folded winning. The entity arm settles them by ONE precedence instead (criterion 5's): for a keyed event, of the definitions the file's identity asserts on that node at its current generation, the one first in
`CODE_KIND_PRECEDENCE` (`type`, `trait`, `impl`, `function`, `method`, `macro`, `constructor`,
`variant`, `constant`, `static`, `variable`, `declaration`, `property`, `field`, `module`, `other`; a kind string
outside the list ranks with `other`, last; a `rigger-domain` `contextgraph` constant beside the kind
constants, which the adapter's entity arm only reads), the lower line, then the lesser kind string, breaking a tie, gives `kind` and `line`; the arm reads that identity's current-generation `node_assertions` row before its ONE `ensure_node` call (a row of another generation, or carrying no `kind` attr, contributing no candidate) and passes the winner, the precedence minimum of the row's pair and the event's pair, as that call's attrs, with no second write, so the
node is a pure function of the generation's definitions, whatever their emit order
(`symbols/events.rs:216-221`). Across identities and for an unkeyed event the node is last-folded as
today, accepted: a production definition is asserted only by its file's one keyed identity
(`Asserter::of`, `sqlite.rs:1866`), and no production code emits the `AliasDefined` that would
resolve a second file onto it. The node is a definition to every CALL-SIDE reader A DATA MEMBER IS NO CALL TARGET lists,
`definition_name` (`:2146-2156`, the trigger `settle_node` reads at `:2136-2141`) among them,
exactly while its winner is a call target; the entity arm reads `definition_name` before and after its
`ensure_node` and runs `definition_retired` when it turns from a name to none, as `settle_node` does when a node stops being a definition, the upgrade and `resolve_proofs` keeping criterion 4's settled-kind gate, so a keyed `module x`
folding onto a node whose winner is `field x` upgrades nothing. A module sharing a leaf with a data member in its file (`mod store;` beside a field `store`) settles the member's kind and is hidden from CALL-SIDE readers until spec 119's ids, `CODE_KIND_PRECEDENCE` unchanged, accepted. So a node holding a fn at one generation and only a field of that leaf at the next demotes
its references and re-resolves its proofs, as a rebuild of that generation gives; criterion 5's generation-2 fixture's batch opens with the shared node's event (the file's only definition, or its least name), since `fold` otherwise settles the node through `retire_unheld_nodes` after the batch's first event (`sqlite.rs:1952-1958`, `:2041-2068`, `:2136-2141`) and the entity arm's read observes nothing. A field or
property shares no id with any other kind once spec 119's nested ids land, nor does a constructor
with its class or a variant with a same-named type; same-path pairs of other kinds (a Rust
`struct Foo;` beside `fn Foo`, overloads, cfg twins, `mod x;` beside `fn x`) still share one.

**A REFERENCE'S CALLER IS ITS FUNCTION, decided here.** A reference's caller is the innermost
enclosing FUNCTION-LIKE definition (function, method, constructor, a macro body; a closure's
enclosing fn) when one encloses it, else the innermost enclosing definition of any kind as
`enclosing_def` (`symbols/extract.rs:676-687`) takes it today, else the file (criterion 2's); `SymRef.enclosing` keeps carrying that caller's name. A `declaration` is excluded
from the caller rule's ranges (criterion 2's, pinned among its hand-built ranges), its range still
feeding `test_regions` (a `#[cfg(test)] fn f();` is `is_test`), so a reference in its signature
takes the next enclosing definition, else the file. So a call in a fn-local const stays the fn's;
a file-level impl header's trait and self-type references stay the impl's (`extract.rs:105-129`,
`model.rs:116-119`); a fn-local impl header's, a fn-local trait body's outside its methods and a
fn-local inline mod's go to the fn, accepted; and a call in a function-local JS `const x = f()`, a
definition today, moves from `x` to its function, and one in a JS constructor-local const to the constructor. A `require(..)` call is no reference, as
today (upstream `tags.scm:76-80`), and the composed queries add none. Every multi-name Go spec (`const_spec`, `var_spec`, `field_declaration`: `const a, b = f(), g()`, `var a, b = f(), g()`, a field `x, y Foo`) makes each name a node, and a reference inside it takes the caller
`enclosing_def`'s tie-break picks among equal ranges, the least name, accepted, so a multi-name field's type reference names its least field. Every reference category keeps its target and
count, and a reference moves onto whichever definition this spec newly makes its innermost enclosing one, function-like first, at the criterion of 6-10 that makes it a definition: a reference no function encloses onto a data definition
(a const or static initializer, a variant discriminant, a package-level `var` initializer, a class field, a property at 8 (every call inside a C# property declaration) or a class-body assignment);
a C# struct, record or enum's field type references onto it at 8 (`registry.rs:66`'s reference starting at the `variable_declaration`'s type, outside the field's `variable_declarator`); a C# destructor's, operator's, conversion operator's, indexer's and local function's body references onto it at 8, off their class or enclosing method; a C# `file_scoped_namespace_declaration`'s and a qualified-name `namespace A.B.C`'s references onto it at 8, off the file (`registry.rs:70` tagging an `identifier` name only); a TS `internal_module`'s, `type_alias_declaration`'s and string-named `module`'s (`declare module "x"`) references onto it at 9, off the file or the enclosing definition; and the references in a JS or TS declarator's class or generator value (`const Foo = class {..}`, `const g = function* () {..}`) outside their methods onto the declarator at 9; all accepted. A field's type reference (a Go
`field_declaration`, a TS `property_signature` or `public_field_definition`) moves from its struct or
interface onto the field at 7 and 9, naming which field holds the type, the field no call target so
an up-walk from the type stops at `<file>::<field>` and reaches the struct through the field's
container (spec 119's `CONTAINS`), save that a call-target definition sharing the field's leaf in its file makes the shared node a call target and the type's up-walk continues through its callers until spec 119's ids, accepted by name, and a Go embedded field and a Go `type_alias` are read-offs because upstream's `(type_identifier) @name @reference.type` (`tags.scm:30`) holds each one's name node, `tree-sitter-tags` keeping one tag per name node (`lib.rs:567-581`), so that reference stays, its caller the field or alias, a field's target its type (one node with it at flat ids when both sit in one file) and an alias's the alias itself, a self-edge the walks treat as recursion, and an alias has no `definition_extents` extent, accepted; but a chained Python assignment's call moves from its
first target to the next enclosing definition at 10; a call in a JS constructor body moves from its
class to `constructor` at 9 (the upstream query skips a method named `constructor`); a C#
constructor's leaf is its class's, so at flat ids its body's caller id is the class's before and
after 8 and the move is unobservable until spec 119's ids; a re-kinded declaration's references (a TS `method_signature`'s or `function_signature`'s type annotation, a bodiless Go `func`'s types) move off the signature to the next enclosing definition at 7 and 9, the declaration being excluded from the caller ranges; all accepted. The changed `SymRef.enclosing` values are a
persisted index change, so `GRAMMAR_TAGS_VERSION` (`model.rs:168`) becomes `ts-tags-v3` with this
rule and the operator's pre-spec index loads as absent (`model.rs:212-214`); no interim binary is
installed against it (the operator binary boundary), so no interim index exists.

**GENERATIONS AND THE REBUILD.** A generation hashes the batch's bytes (107), not
`GRAMMAR_TAGS_VERSION`: a batch this binary reproduces (an empty boundary, a file whose definitions
and callers are unchanged) keeps its entry, and for every changed batch 107's rule applies verbatim:
"after a change of extractor, grammar or payload no source reproduces an older entry's generation,
every such entry folds nothing at a rebuild, and the derived layer returns through the next
ingest". The new kinds fold through the existing entity arm as today's do, but the shared-node
precedence changes how existing events fold (a file's two `fn new` take the lower line where they
took the last folded), the change `PROJECTION_VERSION`'s doc marks (`sqlite.rs:98-103`), so it
becomes 2 with that rule (criterion 5's) and `rigger setup` rebuilds an older `graph.db`; a batch
this binary reproduces then re-folds under the rule at that rebuild. Criterion 13's store is hand-written (an `index.json` carrying `"grammar":"ts-tags-v2"`, a `graph.db` whose `user_version` is the literal 1, one entry minted by the binary under test and one hand-keyed), its tree-0 red the missing kinds, not the rebuild; it holds a `ts-tags-v2` index, one entry this binary reproduces and one it does not (a Rust file holding a static, a variant, two fields, a `macro_rules!`, a trait required fn and a call naming the second field's leaf, the fn it steps to in a second file sharing that leaf): the first keeps its entry,
the second folds nothing at the rebuild and its file returns at the next ingest. Criterion 13 shows each new kind at a leaf no other
definition in the built store shares (`locate` being LOOKUP, a shared leaf answers its candidates and no kind), the first field shown and the second walked, the second's presence read from `locate`'s candidates for its shared leaf, `constructor` by a JS file whose one class holds a `constructor`
method, and never a C# constructor, whose leaf is its class's at flat ids. After landing the operator runs `rigger setup` and `rigger graph build`; expected then, the
build re-recording every changed batch: about 6,570 extracted definitions (4,550 + ~2,023 Rust
production), counted at the `extract` seam; the node count is lower by every shared node, tuple
fields `<file>::0`, `<file>::1` of every tuple struct and variant in a file collapsing onto one node each until spec 119's ids, accepted.

**UNIT ORDER AND BASE, decided here.** Launched on rigger-run after spec 107 lands, the binary is
refreshed and the store is migrated (`rigger reset --derived`). Criteria land 1 to 14, each needing
the one before; ARTIFACT OWNERSHIP (Notes) gives every artifact one owner, which is its first
production caller's criterion, and the landing simulation is read off it: no criterion's assertion
touches an artifact whose owner lands after it. This spec touches 107's `kind_str` (its new arms);
107's batch and rebuild tests compute their generation through the binary or pin the extraction
tree's (`tests/common/fixtures/extraction_tree.rs` `WALKED`), whose source holds two fns and two `#[cfg(test)]` mods, no new kind, dev-only path, `_` or moved caller, so a batch-changing criterion (1, 2, 3, 6-11) edits none of them. A criterion changing a signature, payload or struct
adapts every caller and constructor in its unit. The audit gate (`tests/simplification_audit.rs`)
holds because each function lands with its first production caller; constants, fields and enum
variants it does not weigh. Each criterion is under the planner's 3,000-added-line cap (UNIT SIZE,
Notes). The caller rule (2) lands before every new data definition, so no interim tree attributes a
call to a fn-local const. At 3 the filter removes a dropped definition from the one range vector
(`def_ranges`, `extract.rs:52`), so its range feeds nothing on trees 3-10; 11 adds the dropped
ranges as a second input to `test_regions`. The extraction criteria 2, 3 and 6-11 assert at the `extract` seam by
`(file, kind, leaf)`, never by node id; a new definition folds through today's
`code_entity_id(file, name)`, a shared node taking one kind (A SHARED NODE TAKES ONE KIND). LANDING TREES (Notes) names the tree each
criterion is red on, the observable that changes, and the tree it is green on; no criterion asserts
the opposite value of an observable an earlier one asserts. Pairs read off the table, each
"yes": 2 before 9 (its constructor clause reads 2's rule); 3 before 11 (11 keeps a filtered definition's range for the
test regions); 6 before 11 (its `const _` and extern block fixtures need 6's Rust patterns); 4 before 5 (5's `definition_name` read reads 4's predicate); 9 before 10 (9 fixes the binding walk's table
shape, 10 adding rows and seeds only); 4 and 5 before 6 (the first field node folds at 6, so no tree holds a field the name-keyed readers
take as a call target or a shared node without its precedence); 6-10 before 12 (the gate reads their lists); 13 asserts on a store 1-12 folded. Criterion 13 is verification-only: its test commits alone with no fix commit, its red evidence the unit's result citing the tree-0 commit and the test's failing output verbatim (in the unit's DecisionMade too), the accepted red
for a verification-only criterion, since 1-12 already turn it green (Global constraints). Trees 6-10 lower the items of a test-attributed extern block as production (a `#[cfg(test)] extern "C" { fn f(); }`'s `f` a production declaration), closed at 11, accepted, no interim binary being installed. A 12 red on a type THE RULE classifies a definition, a 13 red on its green tree, or a red assertion in 6-11 of an artifact an earlier criterion owns (any ARTIFACT OWNERSHIP row an earlier criterion owns), halts the finding unit as an escalation naming the owning criterion and is never patched in the finding unit, a later criterion adding arms or rows to `def_leaf`, `is_declaration` and the binding table but never changing their call position or signature; a 12 red on a type THE RULE classifies a non-definition is 12's in-unit addition (SCOPE).

**CONSTRAINTS WALK, decided.**
- *Empty:* a production file with no definition stamps the empty boundary (spec 86); a reference no definition encloses has the file as caller.
- *Repeated:* an unchanged file re-records nothing (107 row 3).
- *Reverted:* A, B, A records three entries (107) and ends on A's nodes.
- *DROPPED:* a definition removed, or newly `#[cfg(test)]`, retires by the generation rule (spec
  101); a call target whose shared node survives only as a non-target kind (`NON_TARGET_KINDS`)
  retires as a definition, `definition_retired` running from the entity arm.
- *Fold order:* the new kinds fold through the existing entity arm; a shared node's kind is the
  precedence winner of its generation in both emit orders, and a reference tiered between a fn and
  a field of its leaf converges through the arm's tier upgrade or keeps its tier, ending as a rebuild gives; a same-file `extracted` edge onto a node later last-folded to a non-target kind keeps `extracted` for unkeyed events (`definition_retired`, `sqlite.rs:2165-2185`, demotes only `inferred`), accepted.
- *Concurrent ingests:* two sinks folding one file meet 107's re-recording rows.
- *Crash-resume:* each batch folds in one transaction; a rebuild resumes as 107 decided.
- *Cold start:* every fact is a `graph.db` projection of the ledger and the tree; a rebuild replays
  generations in order, each settling the same precedence winner.
- *Existing data:* "fold as today" covers hand-built and unkeyed events only; a pre-spec entry of a
  changed batch folds nothing at a rebuild (107's rule) and its file returns at the next ingest, and
  an unchanged batch's entry folds as today.

**STATE PLACEMENT.** Definitions are `graph.db` projections of the ledger and the tree, every node
keyed by its file's identity and generation; the batch is a pure function of one file's bytes and flag (107).

## Notes (non-criteria)

ARTIFACT OWNERSHIP. Owner = the criterion of the artifact's first production caller (for a field,
table or constant, its first production reader or writer); "-" = not retired.

| Artifact | Owner | First production caller | Tree it first exists on | Retired by |
| --- | --- | --- | --- | --- |
| `events::is_dev_only`, the per-pattern extension gates | 1 | `extract_events`, `proof_events`, `doc_links`, `scored_hits` | criterion 1 | - |
| `is_under_tests_dir` | - | - | base (107) | 1 |
| the function-like caller filter in `enclosing_def` with its `declaration` exclusion, `Kind::Constructor`, `Kind::Macro`, `Kind::Declaration`, their `kind_str` arms and kind string constants | 2 | `extract` | criterion 2 | - |
| the name filter (empty in every language, `_` in Go and Rust) in `extract` | 3 | `extract` | criterion 3 | - |
| `GRAMMAR_TAGS_VERSION` `ts-tags-v3` | 2 | `SymbolIndex::is_current` | criterion 2 | - |
| `contextgraph::is_call_target`, `contextgraph::NON_TARGET_KINDS`, every code kind constant but `constructor`, `macro`, `declaration`, `static`, `variant` and `variable` (rigger-domain), `call_target_sql` (rigger-graph-sqlite), `definitions_with_suffix`'s `call_side` argument, every CALL-SIDE row of the reader table | 4 | every CALL-SIDE site of the reader table, `Projector::locate`, `kind_str` | criterion 4 | - |
| `CODE_KIND_PRECEDENCE` with the `static`, `variant` and `variable` kind constants, the keyed shared node's kind and line, the entity arm's before-and-after `definition_name` read | 5 | the entity fold arm | criterion 5 | - |
| `PROJECTION_VERSION` 2, made public | 5 | the projection open | criterion 5 | - |
| `RUST_TAGS` definition patterns, read-off tuple fields, proc-macro kind, the trait lowering, the `extern crate` alias leaf, `extract::is_declaration` with Rust's type | 6 | `extract` | criterion 6 | - |
| `Kind::Static`, `Variant`, `Field`, their `kind_str` and `kind_of` arms, the `macro` `kind_of` arm | 6 | `extract` | criterion 6 | - |
| the attribute walk from the outermost node below the root, the tuple field's type-node walk, the inner-only count of `leading_inner_test_attribute` | 6 | `preceded_by_test_attribute` | criterion 6 | - |
| addendum `context-management.md:199` | 6 | - | criterion 6 | - |
| Go query, the embedded-field and `type_alias` read-offs, Go's declaration types, `Kind::Variable` with its `kind_str` and `kind_of` arms | 7 | `extract` | criterion 7 | - |
| `CSHARP_TAGS` patterns, read-off operators and indexers, the C# leaf shapes, `extract::def_leaf` with its destructor arm, C#'s declaration types, `Kind::Property` with its arms, the `constructor` `kind_of` arm | 8 | `extract` | criterion 8 | - |
| JS/TS query additions, read-off fields and enum members, a function-valued field as `method`, private, string and numeric member leaves, `def_leaf`'s quoted-name arms, computed names excluded, the JS/TS `constructor`,  the declarator kind rule, one composed query per grammar with `.tsx`'s own `for_extension` entry, the binding walk and its JS and TS table rows, the for-in identifier binding, the constant enumeration's check against the `expression` supertype, TS's declaration types | 9 | `javascript_tags_query`, `typescript_tags_query`, `extract` | criterion 9 | - |
| Python query with its module-level pattern's `right` filter, the method rule, read-off class-body assignments, Python's binding-table rows and seeds (pattern and chained targets) | 10 | `extract` | criterion 10 | - |
| each language's `#[cfg(test)]` `<LANG>_DEFINITION_TYPES` constant | 6-10 | test only | criteria 6-10 | - |
| test regions over a name-filtered definition's, a `foreign_mod_item`'s and a `#![cfg(test)]` file's range | 11 | `test_regions` | criterion 11 | - |
| the gate test, each language's `#[cfg(test)]` `<LANG>_NON_DEFINITIONS` constant | 12 | test only | criterion 12 | - |

UNIT SIZE (added lines, code and tests, estimated as 107 did): 1 ~300; 2 ~300; 3 ~100; 4 ~350;
5 ~300; 6 ~450; 7, 8 and 10 ~300 each; 9 ~700 (three queries, the `.tsx` entry, the declarator, binding-walk
and for-in rules, the constructor kind, the member-leaf shapes, three supertype checks); 11 ~200; 12 ~250; 13 ~150. Every unit is under the 3,000-line cap.

DEFINITION TYPES. Read from `NODE_TYPES` of tree-sitter-rust 0.24.2, -go 0.23.4, -c-sharp 0.23.5,
-javascript 0.23.1, -python 0.23.6 and `TYPESCRIPT_NODE_TYPES`/`TSX_NODE_TYPES` of -typescript
0.23.2. "Named" as SCOPE defines it; each definition's leaf per THE LEAF; read-off = taken from the parse tree by a named function.
THE RULE, which reproduces both lists and classifies a type the gate finds unlisted: a named type is
a definition unless it is a parameter or argument, a pattern, binding or designation (a tuple
element included), an expression, a type reference, a path or qualified name, an import, export or using directive
(an `extern crate`, which declares a module name in its file, is a definition; C#'s
`extern_alias_directive` only aliases an assembly), an attribute, an accessor, a query clause, a JSX
element, an index, mapped-type or predicate signature, or a container body.

| Language | Definition types (kind) | Declared non-definitions |
|---|---|---|
| Rust | `function_item` (function/method/macro), `function_signature_item` (declaration), `struct_item`, `union_item`, `enum_item`, `type_item`, `associated_type` (type), `trait_item` (trait), `mod_item`, `extern_crate_declaration` (module), `const_item` (constant), `static_item` (static), `enum_variant` (variant), `field_declaration` (field), `macro_definition` (macro); read-off: `impl_item` (impl), `ordered_field_declaration_list` members (field, by index) | `const_parameter`, `lifetime_parameter`, `type_parameter`, `field_pattern`, `token_binding_pattern`, `scoped_identifier`, `scoped_type_identifier`, `struct_expression`, `type_binding`, `foreign_mod_item` (an extern block: a container, like a block) |
| Go | `function_declaration` (function), `method_declaration` (method), either holding no `body` a declaration, `method_elem` (declaration), `type_spec` (type), `const_spec` (constant), `var_spec` at package level, its `var_declaration` a direct child of `source_file`, grouped specs included (variable), `field_declaration` with a `name` (field); read-off: an embedded `field_declaration`, one with no `name` (field), `type_alias` (type) | `package_clause` (a container name), `import_spec`, `parameter_declaration`, `variadic_parameter_declaration`, `type_parameter_declaration`, `qualified_type` |
| C# | `class_declaration`, `struct_declaration`, `record_declaration`, `interface_declaration`, `enum_declaration`, `delegate_declaration` (type), `enum_member_declaration` (variant), `method_declaration`, `destructor_declaration` (method), `local_function_statement` (function), `constructor_declaration` (constructor), `property_declaration`, `event_declaration` (property), `namespace_declaration`, `file_scoped_namespace_declaration` (module), `variable_declarator` under `field_declaration` (field, or constant with `const`) and `event_field_declaration` (field); read-off: `operator_declaration`, `conversion_operator_declaration`, `indexer_declaration` (method); a `method_declaration`, `destructor_declaration`, `local_function_statement`, `constructor_declaration`, `operator_declaration` or `conversion_operator_declaration` holding no `body` a declaration | `accessor_declaration`, `parameter`, `type_parameter`, `catch_declaration`, `declaration_expression`, `declaration_pattern`, `var_pattern`, `list_pattern`, `recursive_pattern`, `tuple_pattern`, `parenthesized_variable_designation`, `tuple_element`, `argument`, `attribute`, `attribute_argument`, `from_clause`, `alias_qualified_name`, `qualified_name`, `member_access_expression`, `member_binding_expression`, `bracketed_parameter_list`, `parameter_list`, `extern_alias_directive`, `using_directive` |
| JS | `function_declaration`, `generator_function_declaration`, `function_expression`, `generator_function` (function), `method_definition` (method), `class_declaration`, `class` (type), `variable_declarator` (function, type or constant by its value; under a pattern name each bound identifier, constant), `method_definition` named `constructor` (constructor); read-off: `field_definition` (field), a `for_in_statement` with a `kind` (its `left` identifier, or each identifier its `left` pattern binds, constant), and as today (upstream `tags.scm:63-74`, `:90-99`, no `name` field) an `assignment_expression` or `pair` whose value is a function (function) and an exported `assignment_expression` (constant) | `export_specifier`, `import_specifier`, `jsx_opening_element`, `jsx_closing_element`, `jsx_self_closing_element` |
| TS/TSX | JS's but `field_definition`, plus `function_signature`, `method_signature`, `abstract_method_signature` (declaration), `abstract_class_declaration`, `interface_declaration`, `enum_declaration`, `type_alias_declaration` (type), `module`, `internal_module` (module), `enum_assignment` (variant), `property_signature`, `public_field_definition` (field); read-off: `enum_body`'s bare `name` members (variant), an `enum_assignment` being its tag's alone | JS's (its `jsx_*` three TSX's only), plus `enum_body`, `index_signature`, `mapped_type_clause`, `generic_type`, `nested_type_identifier`, `type_parameter`, `type_predicate`, `optional_parameter`, `required_parameter` |
| Python | `class_definition` (type), `function_definition` (method in a class, else function); module-level single-target assignment (constant, a tag as today); read-off: class-body assignment (field), each bare identifier a pattern or chained target binds (its target's kind), `type_alias_statement` (type) | `aliased_import`, `future_import_statement`, `import_from_statement`, `import_statement`, `default_parameter`, `typed_default_parameter`, `keyword_argument`, `named_expression` |

A read-off entry names the members it reads, never its container (`enum_body` is a declared
non-definition, its members variants). Both columns live inside `registry.rs`'s column-0 `#[cfg(test)]` test module (`registry.rs:252-253`), made `pub(crate) mod tests {` with each constant `pub(crate) const` so criterion 6's tests in `extract.rs` read them,
as one constant pair per language, so a commit adding or changing them is a test commit to `.rigger/gates/red-before-green.sh` (its test module running from that line to the end of the file, `:6-11`, `:41-55`, the pattern `/^(pub(\([a-z]+\))? )?mod [^;]*[{]/` at `:45` matching `pub(crate) mod tests {`), `<LANG>_DEFINITION_TYPES` and `<LANG>_NON_DEFINITIONS`, each `<LANG>_DEFINITION_TYPES` row a (node type, kind, fixture leaf) triple, 6-10's tests asserting one `(kind, leaf)` `Def` per row and 12's gate reading the type column of both; no production code reads them. A grammar type added
without a `name` field is outside the enumeration and found by review, accepted.

GATE DRY RUN. THE RULE's enumeration over the six pinned `node-types.json` files (under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`) finds Rust 24, Go 13, C# 40, JS 13, TS
31, TSX 34 and Python 10 named types, 0 absent from the lists; an unlisted non-definition found
later is added by criterion 12 to its own `<LANG>_NON_DEFINITIONS`.

DEV-ONLY PATHS. Ungated, in every language: a directory component `tests`, `benches`, `examples` or
`__tests__`. Gated on the language `registry::for_extension`
(`registry.rs:177-211`), the one extension-to-language table, gives the file's extension: Go
`*_test.go`; Python `test_*.py` and `*_test.py`; JS/TS `*.test.*` and `*.spec.*`; C# a file under a
directory component whose last dot-separated segment ends in `Test` or `Tests`. The gate reads the
path's own extension, so a file indexed under a `--language` override whose extension is
unregistered (`registry::for_path`, `registry.rs:223-248`) matches the ungated patterns only, accepted.
Production controls: `src/testsuite.rs`, `build.rs`, `Contest/a.cs`, `LoadTest/run.py`,
`FooTests/lib.rs`, `src/a.test.rs`.

KINDS. `function`, `method`, `constructor`, `type`, `trait`, `impl`, `module`, `constant`,
`static`, `variable`, `variant`, `field`, `property`, `macro`, `declaration`; `other` only for an unknown tag category.

OUT OF SCOPE. A held `gc` identity whose file the tree and the symbols index no longer hold (237
definitions under `src/dash.rs::*` and `src/grounder/symbols/*`) stays live. `scored_hits` treating
an `out_of_line_test_module_files(idx)` target as dev-only is deferred: ranking is grounding's
concern and omits nothing from the graph, so such a target ranks as today.

LANDING TREES. Per criterion, the tree it is red on with the observable that changes, and the tree
it is green on. A clause asserting an absence only the criterion's own new pattern could produce
(the both-inputs file, a Go function-local `var`, a C# method-local variable, a computed member,
`self.x = 1`, an assignment under `if TYPE_CHECKING:`, and criterion 13's down walk stepping past the field, green on tree 0 where no field node exists and red on tree 12 without criterion 4's predicate) is a guard: green on the red tree, red on the green
tree with the pattern but without its exclusion; a clause asserting what the red tree already gives (criterion 4's LOOKUP clauses, criterion 11's `#[cfg(test)] use x;` file and `#[cfg(unix)] fn f`) is a regression guard, green on both trees.

| Criterion | Red on (the observable that changes) | Green on |
|---|---|---|
| 1 dev-only | 0, the base (a `benches/b.rs` and a `x_test.go` lower their definitions to nodes, `scored_hits` marks a `*_test.go` location production, `doc_links` keeps a link to `pkg/x_test.go`, a `benches/b.rs` file's references emit no proof) | 1 |
| 2 caller | 1 (a call in a function-local JS `const x = f()` has caller `x`; a fn-local impl header's, trait body's and inline mod's references have the impl, trait or mod; a reference inside a hand-built declaration's range has the declaration as caller; a `ts-tags-v2` index is current) | 2 |
| 3 name filter | 2 (a Go `func _()` yields a `Def` `_`, and a call inside it has caller `_`, since the filter drops the definition from the caller rule's ranges as well as from `defs`) | 3 |
| 4 data member | 3 (a hand-built field node is a definition to every name-keyed reader: a same-file call naming its leaf resolves onto it, it joins the walk's frontier, and `callers_of` at it returns the calls naming its leaf) | 4 |
| 5 shared node | 4 (a fn above a field of its leaf settles `field`, the last folded winning; a node holding a fn at generation 1 and only a field at generation 2, the field its batch's first event, leaves a cross-file `inferred` reference `inferred`; a `PROJECTION_VERSION` 1 `graph.db` owes no rebuild) | 5 |
| 6 Rust | 5 (no `Def` for a const, static, variant, field, trait required fn, associated type or `extern crate`; `macro_rules!` lowers `other`; a Rust trait lowers `type`; a fn, mod, struct, enum or impl whose body opens with a `#[cfg(test)]` child is test; a `#[proc_macro]` fn lowers `function`) | 6 |
| 7 Go | 6 (no `Def` for a const, a package-level var, a field, a `method_elem` or a `type_alias`; a bodiless `func` is a `function`; a field's type reference has its struct as caller) | 7 |
| 8 C# | 7 (no `Def` for a struct, enum, record, field, property, event, constructor or destructor; `namespace A.B.C` and an operator yield no `Def` at THE LEAF's leaf; an interface method is a `method`) | 8 |
| 9 JS/TS | 8 (no `Def` for a TS enum, type alias, class field, `const X = {..} as const`, `const Foo = class {}`, `const g = function* () {}` or a destructured binding, a string or numeric member, `declare module "x"` or a JS `constructor`; a TS `method_signature` is a `method`; a call in a JS constructor body has its class as caller; a `property_signature`'s type reference has its interface as caller) | 9 |
| 10 Python | 9 (a method in a class lowers `function`; `x = y = f()` tags `x` over the whole chain, `f`'s caller `x`; no `Def` for a tuple or chained target's second name; no `Def` for a class-body assignment or `type X[T] = int`) | 10 |
| 11 test regions | 10 (a fn inside a `#[cfg(test)] const _` block and the items of a `#[cfg(test)] extern "C"` block are production; a second fn in a file opening `#![cfg(test)]` is production, and a `mod x;` in it is outside `out_of_line_test_module_files`) | 11 |
| 12 gate | 11 (no test fails when a named type is absent from every list) | 12 |
| 13 older store | 0 (the hand-written base-shape store folds no new kind at `rigger graph build`); verification-only, 1-12 turn it green | 12 |
| 14 lanes | - | 14 |

TEST DISPOSITIONS. Every test named was read at rigger-run (a070ffdd); a row with no test says the
search it ran.

| Test | First broken by | Disposition |
|---|---|---|
| spec 86's exclusion tests: `symbols/events.rs` `extract_events_excludes_a_file_under_a_tests_directory_entirely` (`:1173`, real extraction of `tests/foo.rs`, `crate/tests/bar.rs`, `src/foo.rs`, `src/testsuite.rs`), `ingesting_a_fixture_with_product_and_test_code_graphs_only_the_product_and_lists_no_test_file` (`:1291`), `a_product_entity_referenced_by_two_tests_carries_proven_by_2_and_renders_on_its_card` (`:1410`), `proof_events_on_a_whole_file_test_directory_with_zero_references_also_returns_the_sentinel` (`:1560`); `design/extract.rs` `a_tests_rooted_path_mention_never_links_inline_or_by_citation` (`:708`); `tests/code_entity_test_exclusion_periphery.rs` `:538` and `a_tests_rooted_design_doc_mention_never_becomes_a_live_node_through_the_public_api` (`:1802`); `tests/proof_lands_on_the_card_periphery.rs` `a_reference_free_tests_dir_files_first_extraction_creates_nothing_and_leaves_no_residue` (`:745`) | 1 | kept: each path is under `tests`, an ungated DEV-ONLY PATHS component, or is a production control, and no fixture file is in both inputs; their calls re-point through `is_dev_only` |
| `a_reference_is_attributed_to_its_enclosing_definition` (`extract.rs:958`, upstream `tree_sitter_rust::TAGS_QUERY` over a hand-written source) | 2 | kept: its impl is file-level, its calls sit in fns, its top-level `setup!()` keeps no caller |
| `rust_impl_blocks_are_definitions_linked_to_their_trait_and_type_and_path_calls_are_references` (`extract.rs:804`, `RUST_TAGS` over a hand-written source) | 2 | kept: its impl headers are file-level; the trait's required `fn append(&self);` becomes a declaration `Def` at 6, which the test's impl filter and `contains` checks never see |
| `rust_grammar_kind_mapping_is_characterized` (`extract.rs:912`, upstream `tree_sitter_rust::TAGS_QUERY`, not `RUST_TAGS`) | 6 | re-expressed: `mymac` lowers `macro` through 6's `kind_of` arm; under the upstream query `Drawable` stays `type`, `MAX` and `GLOBAL` stay absent and the count stays 8 |
| `js_plain_const_declarations_extract_as_constant_definitions` (`registry.rs:355`, the composed JS query) | 2, 9 | kept: it pins kinds and one `greet` `Def`, both kept at 9 (its enumeration excludes `arrow_function`); its top-level consts' calls keep the const as caller, no function enclosing them |
| `every_registered_language_resolves_and_extracts_a_definition` (`registry.rs:329`) | - | kept: an `any` check over one top-level definition per language; the gate sits beside it |
| the count assertions `tests/console_map_frame_wire_shape_periphery.rs:92`, `tests/dash_cluster_detail_drill.rs:111`, `tests/dash_graph_exploration_overview.rs:249-272` | - | kept: each builds a hand-built `Graph` literal (`:58-76`, `:101-111`, `:249-261`), no extraction |
| real-extraction graph tests over Rust: `tests/code_entity_test_exclusion_periphery.rs` `ingesting_product_and_test_code_through_the_public_api_graphs_only_the_product` (`:538`, fns and a test mod only) and `cfg_test_on_every_other_item_kind_excludes_or_stays_scoped_through_the_public_api` (fixture `:1530`) | 6 | kept: the first fixture holds no new kind; in the second, `TEST_CONST` and `TEST_STATIC` become `Def`s with `is_test` true at 6, so its absent list and its `product` node hold |
| `ground_ranked_puts_the_impl_sites_of_an_impl_shaped_query_above_incidental_word_matches` and `ground_ranked_puts_the_entity_matching_the_most_query_words_across_name_and_path_first` (`grounder.rs:1543`, `:1575`, real extraction of `store_tree`, `:1503-1530`) | 6 | kept: the trait's required `fn append(&self);` (`:1507`) becomes a declaration in `domain/eventstore.rs` matching `impl EventStore for` by its path alone, below every impl the test positions; `sqlite store open` does not match it; `scored_hits` weighs every `Def` alike (`grounder.rs:490-492`) |
| `arm_a_safe_view_text_searches_exactly_the_spans_the_index_cannot_resolve` (`blast_radius_eval.rs:350`, corpus `build_adversarial_corpus`, `:298-343`) | 6 | kept: `painter.rs`'s `draw` becomes a definer, and the test asserts only the `on_reload` superset, the `render` class (`macros.rs`'s `macro_rules!` defines `paint`, never `render`) and the precise view inside the safe one for every query |
| a test pinning a JS or TS destructured binding, a Python tuple or chained target, a class-valued or generator-valued declarator as no definition or at another kind | 9, 10 | none in the tree: a search of every test string literal for `const {`, `const [`, `let`/`var` patterns, a for-of pattern, `a, b =` and `x = y =` in a JS, TS or Python fixture finds none |
| a test pinning a definition named `_` | 3 | none in the tree: a search of every `.rs`, `.go`, `.py`, `.js`, `.ts` and `.cs` file for `const _`, `fn _(`, `func _(`, a method's `) _(`, `type _ `, `def _(`, `class _` or a line-leading `_ = ` finds only `tests/dash_exploration_route_client_contract.rs:285`'s compile-time `const _: () = assert!(..)`, test code, no fixture |
| a test pinning the body read's outer-attribute count | 6 | kept: a search of every `.rs` file for a body opening (after comments) with `#[test]` or a `cfg` naming `test` finds `tests/code_entity_test_exclusion_periphery.rs` `:2164` and `:2235`, whose `mod outer { .. }` (`:2235`: `mod outer { mod middle { .. } }`) asserts nothing of the body's owner (`outer`, `middle`), which turns production, `helper` staying test by its own attribute; `:2452`'s fixture steps around the old read with a leading statement |
| a test pinning a shared node's kind or line within one keyed generation | 5 | none in the tree: the keyed fixtures (`contextgraph/sqlite.rs` `apply_keyed`'s callers `:11415-11719`, `tests/compaction_generations_periphery.rs:333`) never fold two definitions of one leaf in one file and generation; the unkeyed ones merge as today |
| a calls-walk test whose call sits in a const or static initializer no fn encloses | 6 | none in the tree: a search of every multi-line and `\n` string fixture in the tree's `.rs` files for a `const` or `static` item finds `TEST_CONST` and `TEST_STATIC` above, whose initializers call nothing, `extract.rs:920-921` (`rust_grammar_kind_mapping_is_characterized`, upstream query), `tests/boundary_audit.rs:861` and `tests/template_sets_build.rs:138` (no extraction) |
| tests pinning the current `PROJECTION_VERSION` 1: `tests/compaction_generations_periphery.rs` `a_rebuild_interrupted_in_its_tail_finishes_the_tail_on_the_next_call` (`:901`), `an_open_owes_the_rebuild_until_it_is_paid_writes_nothing_and_an_unfolded_file_owes_none` (`:1122`, `:1126`), `rigger_setup_rebuilds_a_release_era_graph_db_from_the_log_and_stamps_the_rule` (`:1329`), `rigger_setup_names_both_causes_a_graph_db_owes_its_rebuild_for_and_pays_both` (`:1381`), `opens_racing_the_rebuild_neither_wait_nor_undo_it` (`:1600`), `an_emit_while_the_rebuild_folds_its_tail_is_folded_at_once_and_met_exactly_once` (`:2092`), each reading a real `graph.db`'s `user_version` against a literal `1` | 5 | re-pointed through `rigger::contextgraph::sqlite::PROJECTION_VERSION`, made `pub` at 5, so a later bump re-pins none; the `PROJECTION_VERSION - 1` fixtures (`contextgraph/sqlite.rs:5501`, `:5525`) and the `user_version = 0` pre-rule fixtures (`tests/compaction_generations_periphery.rs:479`, `:959`, `src/cli/mod.rs:5195`) kept, the second test's pre-rule reads (`tests/compaction_generations_periphery.rs:1105`, `:1110`) comparing `0`. Search: every `.rs` file for `PROJECTION_VERSION` and `user_version` |
| fixtures at a newly dev-only path | 1 | kept: matching the tree's paths outside `tests/` against DEV-ONLY PATHS finds `shim/shim.test.mjs` and `examples/demo/` (markdown and YAML only), and a search of every `.rs` string literal for a path the patterns newly match finds `examples/demo` loaded as configuration (`tests/scaffold_grounder_resolves.rs:123`, `config_store.rs:456`, `:2090`, `src/cli/mod.rs:8301`) and a shipped-files message (`src/cli/mod.rs:9288`), no extraction fixture; the tree's own `shim.test.mjs` leaves this repository's graph at its next ingest, accepted |
| a test pinning a caller criterion 2, 7, 8 or 9 moves | 2, 7, 8, 9 | kept: a search of every test for `.enclosing`, `enclosing:` and `caller:` finds, beyond the three rows above, `tests/symbol_ref_caller_attribution.rs` `extractor_attribution_survives_the_build_index_save_load_pipeline` (`:133`: a call in a fn, a file-level impl header and a top-level macro call, none moved) and hand-built `SymRef` and `EdgeInferred` values no extraction re-derives (`contextgraph.rs:1063`, `:1089`, the `events.rs` and `model.rs` tests, `grounder.rs:2211`, `tests/common/symbol_index.rs:28`, `tests/code_ingest_events.rs:514`, `tests/common/fixtures/fold.rs:179`, `:261`, `tests/community_detection_cli.rs:66`, `tests/code_entity_test_exclusion_periphery.rs:165`), and the extraction tree's pinned `"caller":"product"` (`tests/common/fixtures/extraction_tree.rs:134`), a call in a fn, not moved; a search of every `.rs` string fixture for a TS `interface`, method or function signature, `declare function`, `abstract` member or a bodiless Go `func` or `interface` method, whose type reference's caller moves at 7 and 9, finds none; a search of every `.rs` string fixture holding C# or TS source (no `.cs`, `.ts` or `.tsx` file sits in the tree) finds `blast_radius_eval.rs:436` (`class Auth { void authenticate() {} }`), `:450` (`export function render() {}`), `registry.rs:335` (`class Widget { public void Draw() {} }`), `:337` and the `Panel.tsx` component (`:300`, `:316`), none holding a C# struct, record, enum, destructor, operator, conversion operator, indexer, local function or file-scoped or qualified namespace, a TS namespace, type alias or `declare module`, or a JS or TS class- or generator-valued declarator, whose references' caller moves at 8 and 9 |
| a test pinning a re-kinded definition (a Python method as `function`, a Rust trait as `type`, `macro_rules!` as `other`) | 6, 10 | none beyond `rust_grammar_kind_mapping_is_characterized` above: a search of every test for `Kind::Other`, `"other"`, `macro_rules!`, `Kind::Trait`, a trait fixture asserting `Kind::Type` and a Python fixture holding a `class` with a `def` finds only `blast_radius_eval.rs:316` (its row above) and `tests/code_entity_test_exclusion_periphery.rs:1546` (a `#[cfg(test)]` `macro_rules!`, excluded at every kind); no Python class fixture exists |
| an exact definition or node count over real Go, C#, TS or Python extraction | 7-10 | kept: a search of every test function holding a `.go`, `.cs`, `.py`, `.ts`, `.tsx`, `.js` or `.mjs` fixture path beside a count assertion finds `ground_ranked_keeps_a_cross_language_reference_as_its_own_in_language_entity_not_absorbed_into_a_foreign_definition` (`grounder.rs:1631`) and its CLI twin (`tests/cli.rs:2854`), whose `shim.mjs` holds one `function unwrap() {}`, no new kind, so their 2 rows stand |
| an exact definition or node count over real Rust extraction under `RUST_TAGS` | 6 | kept: a search of every test running extraction (`extract(`, `build_index`, `Symbols::open`, `RUST_TAGS`, `extract_events(`, `project_events`, `ingested_subgraph`) beside a count assertion finds, beyond the rows above (`extract.rs:804`, `:912`, `registry.rs:355`, `grounder.rs:1631`), `events.rs:1094` (hand-built), `grounder.rs:1292`, `:1601`, `:1894`, `:1933`, `:1962`, `:1995`, `tests/code_entity_test_exclusion_periphery.rs:1184`, `tests/code_ingest_events.rs:732` and `tests/migration_is_deliberate_periphery.rs:76`, `:194`, `:241`, `symbols/mod.rs:495`, `:510`, `tests/compaction_generations_periphery.rs:3232`, `tests/dedup_seeding_periphery.rs:610`, `:757` and `tests/group_lookup_periphery.rs:402`, whose literal sources hold no const, static, enum, field, trait, type alias, `extern crate` or `macro_rules!` |
| a real-extraction blast-radius test over a `const` fixture: `blast_radius_text_search_matches_a_trailing_underscore_span_as_a_prefix`, `blast_radius_text_searches_only_the_spans_the_index_does_not_resolve` and `blast_radius_text_search_skips_specs_and_the_audit` (`grounder.rs:1173`, `:1204`, `:1228`, fixtures `:1176-1177`, `:1208`, `:1231`) | 6 | kept: each `const` becomes a `constant` `Def` at 6, and no query names its leaf (`TYPE_`, `RunStarted`, `unindexed_name`), so each span stays unresolved and its exact safe view the text search's; the initializers call nothing. Search: every `.rs` file 107 changed for a `const` or `static` item in a string fixture beside real extraction |
| a test pinning the literal `ts-tags-v2` | 2 | none in the tree: a search of every `.rs` file finds only `GRAMMAR_TAGS_VERSION` itself (`model.rs:168`) |
| a real-extraction proof test whose proven leaf a new call-target kind shares in another file | 6-10 | kept: a search of every test reading `proven_by`, `resolve_proofs` or `proofs` beside real extraction finds `contextgraph/sqlite.rs:11035`, `symbols/events.rs:1292`, `:1410`, `:1508`, `:1736`, `:1780`, `tests/live_project_ingestion.rs:46`, the conductor's proof tests and `tests/proof_lands_on_the_card_periphery.rs:94`, `:199`, `:417`, `:592`, `:687`, `:746`, whose product fixtures hold fns only |
| a real-extraction walk or proof test whose trait or interface and its implementation sit in different files | 6-9 | none in the tree: a search of every test string fixture for a bodiless `fn ..;`, `interface`, `abstract` or a TS signature finds only `blast_radius_eval.rs:323`, `extract.rs:806`, `:915`, `grounder.rs:1507` and `simplification_audit.rs:7008`, none in a file reading `calls_down`, `callers_of`, `--calls`, `proven_by`, `resolve_proofs` or `proofs` |
| 107's batch, rebuild and sink tests | - | kept: a search of every `.rs` file for a literal generation finds the extraction tree's `WALKED` (`tests/common/fixtures/extraction_tree.rs:107-238`), which the walk test (`rigger-grounder` `ingest.rs:1390-1394`) and the conductor's sink tests (`conductor.rs:21097`, `:21697`'s `gc/src/lib.rs@f81a57a5c4f55f52#4`) compare with the binary's, its batches unchanged by every criterion (its `src/lib.rs` holds `product`, `helper`, `#[cfg(test)] mod checks;` and a `#[cfg(test)] mod inline`), hand-written keys no test compares with a minted one (the `@h1`-style keyed fixtures, `@abc123` in `dedup_seeding_periphery.rs:147-148`, `@deadbeef` in `rigger-domain` `ingest.rs:461` and `tests/published_content_key_split_periphery.rs:84`, a key naming no identity in `rigger-grounder` `ingest.rs:2249` and `tests/run_sink_periphery.rs:636`) and `batch_generation`'s own hash pins (`rigger-grounder` `ingest.rs:1409-1425`); every other batch test computes its generation through the binary |

DOCUMENT EDITS, each to the criterion whose change makes it false: `docs/architecture-addendum-
context-management.md:199` ("code-entity tree-sitter (fn/type/mod)") to criterion 6, the first producer of a kind outside fn, type and mod, naming the KINDS vocabulary as the kinds the extractor may emit, ahead of its producers on trees 6-7,
accepted, so 6-10 leave it true. No skill or `docs.rs` passage names code kinds.

## Global constraints

- Hyphens, never em or en dashes, ASCII only, in every added line.
- A line citation names the symbol at the tree it was read on (a070ffdd, main after spec 107); the
  symbol name governs, the line being a pointer only.
- Criterion 13 is verification-only: its test committed alone, with the tree-0 commit and the test's failing output cited verbatim in the unit's result and DecisionMade, satisfies red-then-green for it.
- Criterion 12 is test-only: its red commit is the `#[cfg(test)]` gate function (a node-types text and both lists to the unlisted named types) with every `<LANG>_NON_DEFINITIONS` empty, failing on every listed non-definition, its green commit fills the constants, and one case over a hand-written node-types text holding one unlisted named type pins that the gate fails; criterion 14 adds no test and owes no red, its evidence the lanes' green output.
- Every run launched while the definer-only window is open (119, 117, 118 and any later run) is partitioned on SCOPE's definer-only safe view of this spec's kinds exactly as every run since PR #73 is for types; a spec that adds reference patterns for a kind narrows the window for that kind when it lands, and none is a precondition for any launch.
- No new event type; no new dependency (`NODE_TYPES` and `serde_json` are present).
- Every lane `.rigger/gates/lanes.sh` runs green (fmt, clippy -D warnings, test).
- Every batch is a pure function of one file's path, bytes and flag; every rebuild deterministic (107).
- No index persisted by an interim tree is read by a later one: every test persists its index in
  its own temp root, so `ts-tags-v3` at criterion 2 covers the query changes of 6-11.
- The light lane compiles no extraction: criteria 1, 2, 3 and 6-13 assert in the default lane;
  criteria 4 and 5 fold hand-built events in the default and light lanes, and the core lane, which compiles neither the grounder nor `contextgraph/sqlite.rs` nor `dash.rs` (`any(feature = "store", not(feature = "core"))`), holds only 4's `community.rs` and `contextgraph/query.rs` clauses.

## Done when

- [ ] a test proves DEV-ONLY CODE MAKES NO NODE: a file any DEV-ONLY PATHS pattern names lowers to no production-kind node and no structural edge while, outside the out-of-line set, its references emit as proof evidence,
  asserted in `symbols/events.rs`'s tests with one file per pattern, every production control
  lowering its definitions, a `src/foo/tests/mod.rs` that `#[cfg(test)] mod tests;` declares
  emitting no proof, and in `symbols/grounder.rs`'s tests with `scored_hits` marking a `*_test.go` location as test,
one use of the one predicate, and in `design/extract.rs`'s with `doc_links` dropping a link to
  `pkg/x_test.go`. This criterion OWNS
  what ARTIFACT OWNERSHIP gives criterion 1; the per-language definitions are criteria 6-10's, NOT this one's.
- [ ] a test proves A REFERENCE'S CALLER IS ITS FUNCTION: a reference's caller is its innermost enclosing function-like definition, else its innermost enclosing definition, else its file,
  asserted in `symbols/extract.rs`'s and `symbols/registry.rs`'s tests: a call in a function-local
  JS `const x = f()` attributed to the function, a file-level `impl Draw for Widget` header's
  references to the impl, a fn-local impl header's, a fn-local trait body's and a fn-local inline
  mod's references to the fn, a top-level reference to the file, the filter pinned for each function-like kind and a `declaration` excluded over hand-built definition
  ranges, and a persisted `ts-tags-v2` index not
  current at `SymbolIndex::is_current`. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion
  2; the name filter is criterion 3's and each language's new data definitions are criteria
  6-10's, NOT this one's.
- [ ] a test proves THE NAME FILTER DROPS A NAMELESS DEFINITION: `extract` keeps no `Def` whose leaf is empty, nor one whose leaf is `_` in Go or Rust,
  asserted in `symbols/registry.rs`'s tests with a Go `func _()`, `func (T) _()` and `type _ int` making no `Def` and a JS
  `const _ = require(..)` and a Python `_ = gettext.gettext` each keeping theirs under today's
  queries, a call inside the Go `func _()` taking the file as caller, and in `symbols/extract.rs`'s tests calling the filter over hand-built leaves per
  language (`_` dropped for Go and Rust and kept for JS, TS, Python and C#; empty dropped for all;
  `x` kept). This criterion OWNS
  what ARTIFACT OWNERSHIP gives criterion 3; each language's definitions are criteria 6-10's, NOT this one's.
- [ ] a test proves A DATA MEMBER IS NO CALL TARGET: every name-keyed reader takes a `field`, `property` or `declaration` node as it takes a bare placeholder, but `callers_of`, which returns none there,
  asserted in `contextgraph/sqlite.rs`'s tests over hand-built events: a same-file call naming a
  field's leaf resolving down to another file's fn, its reference tiered as with no field present, a
  field or declaration in one file sharing the leaf of a fn in another adding no candidate to the walk's frontier, no proof moving onto a field,
  a constant of that leaf still a candidate, the up walk from that other file's fn reaching the same-file call's caller, a fn retiring beside another file's field of its leaf demoting its cross-file references as with no field present, `callers_of` returning none at a field as seed and as a node the walk reaches, and `locate` answering the field's leaf;
  and in `rigger-domain`'s `community.rs` and `contextgraph/query.rs` tests, `Coupling::from_graph` and the files lens never resolving a placeholder onto a field, a field keeping its own coupling edges, while the lens folds the field under
  its own file, and in `dash.rs`'s, `reproject_files` folding a field under its own file and never resolving a placeholder onto it; in the conductor's, `write_code_neighborhood` listing no field; and in
  `contextgraph/sqlite.rs`'s tests `call_target_sql()` and `is_call_target`
  agreeing over every kind constant tree 4 holds and an absent `kind`. This criterion OWNS what ARTIFACT
  OWNERSHIP gives criterion 4; the shared node's kind is criterion 5's, NOT this one's.
- [ ] a test proves A SHARED NODE TAKES ONE KIND: a node one file's definitions share under a keyed event takes its `CODE_KIND_PRECEDENCE` winner's kind and line and is a definition only while that winner is,
  asserted in `contextgraph/sqlite.rs`'s tests over hand-built keyed events: a fn above a field of
  its leaf and a field above a fn both settling `function` at the fn's line, a same-file and a
  cross-file reference tiered alike in both orders, a class above its same-named constructor
  settling `type`, two `fn new` settling the lower line, a node holding a fn at generation 1 and
  only a field at generation 2 turning a cross-file `inferred` reference `ambiguous` and moving its
  proof off as a rebuild of generation 2 gives, an unlisted kind beside a `function` settling `function`, two unlisted kinds at one line settling the lesser kind string, a `module x` onto a `field x` winner upgrading no reference, an unkeyed
  re-fold moving `line`, and a `graph.db`
  folded at `PROJECTION_VERSION` 1 owing its rebuild. This criterion OWNS what ARTIFACT OWNERSHIP
  gives criterion 5; the call-target predicate is criterion 4's, NOT this one's.
- [ ] a test proves RUST DEFINITIONS ARE NODES: `extract` under `RUST_TAGS` yields every Rust definition type of DEFINITION TYPES with its kind,
  asserted in `symbols/extract.rs`'s tests over one fixture holding an item, associated and
  fn-local const, an item and fn-local static, unit, tuple and struct variants, named and tuple
  fields (tuple fields at leaves `0` and `1`), a trait required fn, an associated type, an `extern crate` with and without an alias (the aliased one at its alias), a
  proc-macro fn, `macro_rules!` and a trait, a fn above a field of its leaf and a field above a
  fn,
  each Def at its own kind, a call in a fn-local const's initializer attributed to
  its fn, one in a const inside a proc-macro fn to the macro and one in an item static's to the
  static, a `#[cfg(test)]` unit variant, field, const, static and `pub` tuple field each a Def
  with `is_test` true, and `struct S { #[cfg(test)] probe: u32, real: u32 }`, `enum E { #[cfg(test)] A, B }` and `impl X { #[cfg(test)] fn t() {} fn real() {} }` keeping `S`, `real`, `E`, `B`, the impl and its `real` production, and `fn f() { #[cfg(test)] let x = 1; }` and `mod outer { #[cfg(test)] mod inner {} }` keeping `f` and `outer` production. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 6; the caller
  rule is criterion 2's and the test regions criterion 11's, NOT this one's.
- [ ] a test proves GO DEFINITIONS ARE NODES: `extract` under the Go query yields every Go definition type of DEFINITION TYPES with its kind and no function-local `var`,
  asserted in `symbols/registry.rs`'s tests over one fixture, an embedded field named by its type's
  base identifier keeping its reference to the type, a `type A = B` alias of kind `type`, a named field's type reference attributed to the field, `const a, b = f(), g()` giving two nodes with `f` and `g` both attributed to `a`, and a package-level `var a = 1` and grouped `var ( b = 2 )` each a `variable` included. This criterion OWNS what
  ARTIFACT OWNERSHIP gives criterion 7; the gate is criterion 12's, NOT this one's.
- [ ] a test proves C# DEFINITIONS ARE NODES: `extract` under `CSHARP_TAGS` yields every C# definition type of DEFINITION TYPES with its kind, a `const` field as constant,
  asserted in `symbols/registry.rs`'s tests over one fixture, a constructor at kind `constructor`,
  no method-local variable a definition, and `namespace A.B.C`, a
  destructor `~Foo`, an operator `+`, an indexer `this` and `implicit operator int` at THE LEAF's
  leaves. This criterion OWNS what
  ARTIFACT OWNERSHIP gives criterion 8; the gate is criterion 12's, NOT this one's.
- [ ] a test proves JS AND TS DEFINITIONS ARE NODES: `extract` under the composed JS and TS queries yields every JS and TS/TSX definition type of DEFINITION TYPES with its kind,
  asserted in `symbols/registry.rs`'s tests over one fixture per grammar (`.js`, `.ts`, `.tsx`), a
  private `#m` method, `"a-b"` and `1` member names, a computed member making no definition,
  `declare module "x"`, `namespace A.B`, a field holding an arrow function as `method`,
  `const X = {..} as const` and a valueless `let y;` as constants, `const Foo = class {}` as type `Foo`,
  `const g = function* () {}` as function `g`, a `property_signature`'s type reference attributed to the field, `const {a, b: c, ...r} = require("x")` and
  `const [x, [y], , z = 1] = f()` making constants `a`, `c`, `r`, `x`, `y` and `z` with `f()` taking the file as caller, `for (const [k, v] of m)` and `for (const j of m)` inside a fn making
`k`, `v` and `j`, `for (q of m)` and `for (o.p of m)` making no definition, a `constructor` included, a call in the `constructor`
  body and one in a const local to it attributed to the constructor, and the constant enumeration
  holding every `expression` subtype of each grammar's node types, flattened through
  `primary_expression`, but `function_expression`, `arrow_function`, `generator_function` and
  `class`. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 9; the gate is criterion 12's and Python's table rows and seeds
  criterion 10's, NOT this one's.
- [ ] a test proves PYTHON DEFINITIONS ARE NODES: `extract` under the Python query yields every Python definition type of DEFINITION TYPES with its kind, a class-body assignment as a field,
  asserted in `symbols/registry.rs`'s tests over one fixture: a method, a decorated method, a def
  nested in a method a function, a def of a class nested in a function a method,
  `a, b = 1, 2` and `[c, *d] = f()` making constants `a`, `b`, `c` and `d`, `x = y = f()` making one `x` and one `y` with the file as `f`'s caller, `definition_extents`
  bounding `z = 1` by the whole assignment and giving `x` none, `w: int` a constant, an assignment under `if TYPE_CHECKING:` making no
  definition, a class-body `e, g = 1, 2` making fields `e` and `g` and a class-body `h: int` a field, and `self.x = 1` making no
  definition, and `type X[T] = int`
  at leaf `X`. This criterion OWNS what ARTIFACT
  OWNERSHIP gives criterion 10; the gate is criterion 12's and the binding walk criterion 9's, NOT this one's.
- [ ] a test proves A TEST REGION IS EVERY TEST-ATTRIBUTED RANGE: a test-attributed definition the name filter drops, a test-attributed `foreign_mod_item` and a file opening `#![cfg(test)]` each make what they hold test,
  asserted in `symbols/extract.rs`'s tests: `#[cfg(test)] const _: () = { fn helper() { real() } }`
  making `helper` and its `real` reference test, `#[cfg(test)] extern "C" { fn f(); }` making `f`
  test, a file opening `#![cfg(test)]` making its second fn and every reference test, and one opening `#[cfg(test)] use x;` or `#[cfg(test)] mod tests;` keeping its fns production, and a `#[cfg(unix)] fn f() {}` staying production beside the `#[cfg(test)]` controls; and in
  `symbols/events.rs`'s, a `mod x;` in such a file joining `out_of_line_test_module_files`. This
  criterion OWNS what ARTIFACT OWNERSHIP gives criterion 11; the Rust kinds are criterion 6's, NOT
  this one's.
- [ ] a test proves COMPLETENESS IS A GATE: every named node type of each registered grammar sits in exactly one DEFINITION TYPES list,
  asserted in `symbols/registry.rs`'s tests reading each crate's `NODE_TYPES`, a type absent from
  the lists failing the gate. This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 12, the non-definition lists included; the
  definition lists and their patterns, an absent type THE RULE classifies a definition included, are criteria 6-10's, NOT this one's.
- [ ] a test proves THE NEW KINDS REACH AN OLDER STORE: over an index and store hand-written in the base's shape, the next `rigger graph build` folds every new kind and an extracted field is no call target,
  asserted in `tests/cli.rs`, under `#[cfg(feature = "symbols")]`, over a `ts-tags-v2` index, two ledger entries, one this binary
  reproduces (kept) and one it does not (folding nothing, its file re-extracted), and ledger-less
  Go, C# and JS files holding a package-level `var`, a property and a class's `constructor` method, by `rigger setup` then `rigger graph
  build`: `rigger graph --show` of each new kind's leaf printing its kind, and the built store's
  down walk from a call naming a field's leaf stepping past the field to another file's fn. This criterion OWNS what ARTIFACT
  OWNERSHIP gives criterion 13, no artifact: the end-to-end assertion only, adding no mechanism, so a red 13 is a defect of the criterion owning the failing artifact; `ts-tags-v3` is
  criterion 2's, NOT this one's.
- [ ] every lane `.rigger/gates/lanes.sh` runs green (fmt, clippy, test). This criterion OWNS what ARTIFACT OWNERSHIP gives criterion 14, no artifact: only the lanes over the integrated result.
