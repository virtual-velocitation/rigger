# 117 - Every reference reaches the graph

**Goal:** a lookup about a reference is answered by the graph. Today the code store resolves calls
and three Rust-only reference kinds, so the graph cannot say which code reads a field, matches a
variant, implements a trait, passes a function by name or calls through a type's path, and it
keeps no doc comment (Notes, observed). Agents then grep. The evaluation of run 050ed244
(`docs/evaluation/20261106-0135-050ed244.md`, read by the rules of `docs/evaluation/README.md`)
counts 56 `grep_fallbacks`: 21 under `grep_fallbacks_by_target.caller`, and 5 lines in the class
`non-call-reference-absent-from-graph` (evidence `progress:46777`, `progress:47202`,
`progress:45456`, `progress:59611`, `progress:45982`: sites that read a constant, assign a field
or match an error kind). This spec moves two figures in the next evaluation of a run on its
finished tree: that class falls to at most 1 line (`progress:59611` matches an error kind of the
standard library, which names no project definition and stays the text scan's), and
`grep_fallbacks_by_target.caller` falls below 21.

## Design

**CAPTURES.** `compile` keeps each `@reference.<kind>` capture's suffix, where today it folds all
into a call: `call` and `send` stay `MentionKind::Call`; `class`, `type` and `interface` become
`Tagged("type")`; `implementation` becomes `Tagged("impl")`; `constant` becomes `Tagged("const")`;
`field` becomes `Tagged("field")`; `import` becomes `Tagged("use")`. A `@qualifier.path` or
`@qualifier.member` capture in the same match is read into `SymRef.qualifier: Option<Qualifier>`
(`name`, and whether the qualifier is a path or a member access). A `@reference.class` capture on
one of the registry entry's `constructors` node kinds (`new_expression` for TypeScript, TSX
and JavaScript, `object_creation_expression` for C#) stays a call, so a proof still credits `new T()`.
The facts layer writes `Mention.name` as `<qualifier>::<leaf>` for a path and
`<qualifier>.<leaf>` for a member access, and as `leaf` without a qualifier; `leaf` is always the
bare leaf, so the IDX_NAME postings stay keyed by leaf. The symbols model's counts read
`FileSymbols::calls`, whose `SymRef.name` stays the leaf, so a type use leaves them in every
language as it does in Rust today. `Symbols::radius` takes its referencer files from call and
`impl` mentions, so an implementing file stays in a trait's blast radius; reference degree,
commonness and scoring count calls alone. When a reference capture's `@name` node has named children, `tags_in` takes the text of its
last named leaf descendant outside the entry's `type_arguments` node kinds (`type_arguments` for
TypeScript, TSX and Go, `type_argument_list` for C#) as the leaf, and the text before it, its
trailing `.` or `::` dropped, as a member or path qualifier when no `@qualifier` capture gives
one: `Ns.IShape` gives `IShape` qualified `Ns`, `List<IShape>` gives `List`, and the row's
type-position patterns stage each type argument. No `Mention.leaf` holds `::`, `.`, `<` or `>`.
Criterion 19's Python file stages its base-class `impl` mention through a fixture tags query;
`PYTHON_TAGS` stays criterion 28's. In an extractor test, a reference "named `Q::L`" is a
`SymRef` with qualifier `Q` and name `L`, read before the facts layer's dedupe.

**NEW TAGS.** `REF_KIND_FIELD` (a member read or written without a call), `REF_KIND_VARIANT` (an
enum variant named outside a call) and `REF_KIND_IMPL` (a trait or type an implementation header
names) join `REF_KIND_TYPE`, `REF_KIND_CONST`, `REF_KIND_USE` and `REF_KIND_MODULE`.

**RUST REFERENCES.** `rust_references` and `extract` stage, in expression and pattern position
(a path in type position stages by the type rule alone):

- Const read: an identifier matching `^[A-Z][A-Z0-9_]*$`, or a scoped path whose leaf holds no
  lowercase letter (`Self::MAX`, `Mode::RO`); and a format string's inline capture, read from the
  format-string argument when it is a string literal: the first argument of `format!`, `print!`,
  `println!`, `eprint!`, `eprintln!`, `panic!`, `format_args!`, `unreachable!`, `todo!`,
  `unimplemented!`; the second of `write!`, `writeln!`, `assert!`, `debug_assert!`; the third of
  `assert_eq!`, `assert_ne!`, `debug_assert_eq!`, `debug_assert_ne!`. `{{` and `}}` escapes and
  names bound as named arguments are skipped; a width or precision `NAME$` capture counts.
- Variant: a scoped path outside a call whose qualifier and leaf start uppercase and whose leaf
  holds a lowercase letter (`InstallOutcome::AlreadyCurrent`, `Located::Many(c) =>`); a call
  `Hop::Frontier(cs)` stays a call. The path `E::V` of a struct literal or struct pattern
  (`Kind::RejectRecurrence { n } =>`) whose two segments start uppercase and whose leaf holds a
  lowercase letter is in expression or pattern position: it stages a `variant` mention of `V`
  qualified `E`, replacing today's `type` row of `E`, and each field it names a `field` mention
  qualified `V`.
- Function value: a scoped path with a lowercase leaf outside a call (`.map(Store::open)`), and a
  bare identifier passed as a call's argument (`.map(entity_block)`) that starts lowercase, that
  `excluded` does not hold and that no binding in scope binds (a parameter, `let`, `if let`,
  `while let`, match arm, `for` or closure parameter pattern), are `MentionKind::Call` at their
  sites; in a re-parsed token tree the scope is the invocation's.
- Field: `x.f` read or assigned without a call, with a member qualifier only when the receiver
  node is exactly `self`; `T { f }` in a literal or pattern, with path qualifier `T`.
- Impl header: every impl block stages an `impl` mention of its self type, and `impl Tr for T`
  one of `Tr` as well, generics stripped, enclosed by the impl definition itself; they replace the
  call rows of `block.targets` and the trait `type` row that `impl_from` encloses by the self
  type today.
- Qualifier: a path's segment before the leaf when it starts uppercase, generics and turbofish
  stripped (`Memo::<K>::new` gives `Memo`); a qualified-self path `<T as Tr>::f` gives `Tr`;
  `Self::f` gives the enclosing impl's self type, generics stripped, and inside a trait body the
trait; a call or field whose receiver
  node is exactly `self` gives a member qualifier: the self type in `impl T` and `impl Tr for T`,
  the trait in a trait's default method. `self.a.f()` and any other receiver chain, a lowercase
  segment (`paths::module_dir`), an enclosing generic parameter and a `Self` whose self type is
  excluded write no qualifier. An alias qualifier (`use a::B as C; C::f()`) is written as `B`. A
  qualifier that `excluded` holds (a prelude name, a standard-library import or alias, a
  standard-library-rooted path) stages no mention, and so does a path whose root segment
  `excluded` holds, whatever the case of its other segments, or a primitive type (`u32::MAX`).
- Tuple index: `x.0` stages no mention.

**OTHER LANGUAGES.** Each registry entry's tags query stages, through CAPTURES:

| Language | Const read | Type use | Impl header | Field | Qualifier | Doc |
|---|---|---|---|---|---|---|
| Python | an `^[A-Z][A-Z0-9_]*$` name read | an annotation's capitalised names | base classes | `x.f` not called | member, the capitalised name before `.f` | the docstring, dedented; the module docstring for the file |
| Go | out of scope | every type identifier in type position | no such syntax | `x.f` not called | out of scope | the query's `@doc` capture |
| TypeScript | an `^[A-Z][A-Z0-9_]*$` identifier read | every `type_identifier` in type position | `implements`, `extends` | `x.f` not called | member, the capitalised name before `.f` | the query's `@doc` capture |
| JavaScript | an `^[A-Z][A-Z0-9_]*$` identifier read | no such syntax | `extends` | `x.f` not called | member, the capitalised name before `.f` | the query's `@doc` capture |
| C# | a member read, as field | every `identifier` or `generic_name` in a parameter, property, return, local, field, generic argument, array element, nullable, cast, `is`, `as` or `typeof` type | the base list, as `@reference.implementation` alone | `x.f` not called, read or assigned | member, the capitalised name before `.f` | the `///` block directly above, XML tags removed |

A Go constant is spelled as any local, so no per-file fact tells a read of one from a read of a
variable. An import outside Rust names a module the extractor cannot place without the language's
module resolution, and a bare member read in C# is a member or a local only by the enclosing
type's members and the method's bindings; both are out of scope, and the later uses of an
imported name stage as calls, types and fields. A function passed by name is staged in Rust alone:
no evaluation line names one outside Rust, and the text scan answers it.

**MACRO TOKEN TREES.** The Rust extractor re-parses each macro invocation's token tree with the
same grammar, first wrapped as a call's argument list, then as items; the first parse with no
error node contributes its definitions, calls and tagged references at their file positions (the
wrapper's prefix length subtracted on the first line only). A token tree neither parse accepts
contributes those of every subtree of its argument-list parse that holds no error node (a
`json!({"k": f(x)})` gives the call `f`). The re-parse recurses into nested invocations. A
re-parsed definition takes its enclosing definition, path parent and test flag from the
invocation's node, and a re-parsed reference stages with the invocation's context: the file's
`use` list for `excluded` and the alias rule, the invocation's enclosing generic parameters,
bindings, and impl or trait for `Self` and `self`. A `macro_rules!` body is a template, no site of a reference, and contributes
nothing.

**ONE TAGGED MENTION PER SITE KIND.** The facts layer keeps one tagged mention per (enclosing
definition, name, tag, test flag), at its first line; calls stay one per site, since a proof's
evidence lists each call line. `x.f` and `T { f }` in one function are two mentions, their names
differing.

**DOC FACTS.** `model::Def` and `FileSymbols` gain `doc: Option<(u32, u32)>`, serde-defaulted
and omitted when `None`; `code_facts` copies it into `Def.doc` and `FileFacts.doc`. A span is the
1-based inclusive lines from the first doc line to the last. In Rust a doc line is `///`, a
`/** */` block or a name-value `#[doc = "..."]`; every other attribute, `#[doc(..)]` included,
and plain `//` lines between them, stay in the span and are dropped by `doc_text`; `//!`, `/*! */`
and `#![doc = ".."]` form the file span. `tags_in` reads a `@doc` capture in a definition's
match into that definition's span, for every language; a language unit writes only its query's
`@doc` patterns. From the facts format unit on, the upstream `@doc` patterns the Go, JavaScript,
TypeScript and TSX queries already hold give those languages their spans, and that unit re-pins
their rows. The text is never stored: `Extract::doc_text(lang, raw)` turns the span's
raw lines into text, splitting on `\n` with a trailing `\r` dropped and dropping every line with
no doc marker; every line inside a `/** */` or `/*! */` block is a doc line, the opener, the
closer and one leading `*` stripped; for a language with no rule it returns the lines trimmed. The span costs two
integers per row; the text would add about 5 MB to a 9.7 MB snapshot that every read loads and
checks whole.

**MODULE ROOTS AT READ.** `ModuleDecl` gains `as_root`, `as_child` and `path_attr`, replacing
`candidates`. `as_root` places the module as a crate root or `mod.rs` file declares it, `as_child`
as any other file does, an inline module's components and its `#[path]` placed by the same two
rules; for a `mod.rs`, `lib.rs` or `main.rs` file both lists are equal. A file is reached when it
is the chosen target of another file's declaration, resolved from the crate roots down, the first
declaration in path order winning. `module_target` uses `as_root` when the declaring file is
reached by no declaration or by one with `path_attr`, and `as_child` otherwise.

**RESOLVER AT READ.** The resolver in `resolve.rs` decides every edge when the view is read, as
today, comparing names only. A Tagged mention resolves through `tagged` with
`reference_candidate_kinds`: `field` to field, property, constant, static and variant; `const`
adds variant; `variant` to variant; `impl` to type and trait. Every tagged link is a REFERENCES link, so an impl header is
never a call. A qualified mention narrows its candidates to the definitions whose parent's leaf
equals the qualifier or whose parent encloses an `impl` mention of that name. Exactly one narrowed
candidate is an EXTRACTED edge in any file; several give a call the placeholder keyed by (file,
qualified name), whose down-hop is the narrowed set, and a tagged mention no edge, whatever files
they sit in. Every placeholder reader outside `resolve.rs` (`composed`'s `resolve_down_hop`,
`callers_of`, `bound_leaf`, `id_binds_leaf` and `Indexed`'s leaf index, and near's members and
`State::leaf_nodes`) binds a qualified placeholder by its leaf and takes its candidates from the
resolver's narrowed down-hop, never by leaf alone. None: a path qualifier that names a project definition (a product type or trait in
the mention's language) falls back to the unqualified rule; a path qualifier that names none makes
no link and no unresolved entry; a member qualifier falls back to the unqualified rule. An
unqualified `field` mention, and a member-qualified one falling back, resolves to field and
property kinds alone and skips the same-file step: one candidate in the view is an INFERRED
edge, several give no edge; a constant, static or variant is reached by a `field` mention only
through a qualified mention's narrowed set. A qualified placeholder's leaf is the text after its
last `::` or `.`. Proofs (`proof`, `call_edge`'s test fallback,
`proving_calls`) narrow by the same rule, and `resettle`'s dirty set adds every qualified mention
whose qualifier equals the leaf of a definition, or the name of an `impl` mention, in a differing
or flipped file. `near.rs`'s `State::landing` also folds every code file holding a qualified call
mention of one of the id's leaves. Each unit whose change alters settled output moves
`SETTLED_GENERATION` and re-pins its golden on its own tree. `Edge` keeps its shape; a reference's
kind is read from its mention; `--around` and the dash draw the new links through their existing
REFERENCES rendering.

**READERS.** `Resolver::referrers(id)` lists, from the IDX_NAME postings of `id`'s leaf
(`mentions_named`), every mention whose own rule's candidate set (after narrowing, the pool and
the kinds) holds `id`: the enclosing definition's site, the label, the line, and `?` when the
mention gave `id` no single edge; a mention with no enclosing definition lists its file node's
site. The label follows the mention's tag:

| Tag | Label |
|---|---|
| call | `called by` |
| `const` | `reads` |
| `type` | `type use` |
| `impl` | `implemented by` |
| `field` | `field use` |
| `variant` | `variant use` |
| `use` | `imported by` |
| `module` | `declared by` |

`ContextGraph` gains `referrers(id, tests)`, test mentions hidden unless tests are shown, and
`doc(&Node) -> Option<String>`. `Composed` answers `doc` through a domain `SourceLines` interface,
which the store's code half implements over the caller's worktree, and `Extract::doc_text`; a span
past the file's end, or a file the read cannot open, gives no doc. `EntitySite` gains
`doc_comment` and `referrers`, filled by `Composed::locate` for a code entity and for a file node;
`site_fields` gives a `doc_comment` entry after `SITE_ATTRS` and, after `unresolved`, one
`referenced by <label>` entry per label holding at most 50 sites joined by `LIST_SEP`, then
`(+N more)`, so `print_entity_site` (`--show`) and the MCP show's `site_object` change no line.
The lookup's `answer` fills `Entity`'s doc and per-label referrers through the same two methods,
and `entity_block` replaces `referenced in` with one line per label under the lookup's existing
caps, plus the doc's first line under the site line. The sidecar's sites carry no doc and no
referrers, and `--around` prints no doc.

**OVERLAY.** A file the caller's worktree changes is re-extracted by the overlay as today, so its
new mentions, doc spans and module lists answer that worktree's reads and no other's; the overlay
gains no rule.

**SNAPSHOT FORMAT.** `snapshot::VERSION` becomes 5: the DEFS and FILES rows gain the doc span,
an absent span written `0,0` and read back as `None`; the MODULES rows gain the two lists and
`path_attr`. A version-4 snapshot fails `Container::parse` and is read as absent, so the next
advance rebuilds from empty, and `FACTS_GENERATION` moves, so the overlay's memo, keyed by the
fingerprint, misses and re-extracts.

**PER-LANGUAGE GENERATIONS.** Each registry entry, TSX included, has one tags generation: a
string literal that a `macro_rules!` macro of its own names, expanded by both the entry's const
and `GRAMMAR_TAGS_VERSION`'s `concat!`; a generation table lists the consts, and a test holds
`GRAMMAR_TAGS_VERSION` equal to the table's consts joined in table order. Its readers in
`grounder.rs` stand. Every addition to the TypeScript entry's query also goes to the TSX
entry's. The registry golden and the facts golden in `facts.rs` each
pin one (language, generation, digest over that language's fixtures) row per registry entry, and the
facts golden keeps one (`FACTS_GENERATION`, digest) row over its design, rationale and workflow
fixtures with its coverage checks. A unit changing one language's extraction moves that
language's generation and re-pins its rows alone; a change to the conversion, design or workflow
rules moves `FACTS_GENERATION` and re-pins that row.

## Global constraints

- Language and process agnostic: every language rule sits behind the extractor interface
  (`Extract`, the registry's per-language entries); the resolver and the readers name no
  language.
- Cross-platform: `SourceLines` opens a worktree file by its path through the standard library
  only.
- Nothing in production code is omitted from the graph; test code is in the graph and hidden by
  default, its referrers included.
- Every lookup and show answers relative to the caller's worktree.
- No external system: everything lives in the owned code store.
- A declared extractor's answer may carry `qualifier`, `doc` and the `field`, `variant` and
  `impl` ref kinds, each serde-optional.
- The unit changing a fact rewrites the section of `docs/architecture-addendum-the-code-store.md`
  stating it, in the same commit: section 4.1 (the leaf), 4.5 (the snapshot rows), 6.2 (the
  resolver rules) and 6.4 (the budget table). Each unit rewrites, in the files it touches, every
  citation of spec 117's earlier criteria or blocks to this text (`paths.rs`'s to MODULE ROOTS AT
  READ, `contextgraph.rs`'s to READERS, `extract.rs`'s to RUST REFERENCES).
- Each unit that changes the code view's answer over `tests/common/fixtures/resolver_tree.rs` adds
  a DEPARTURES row in `crates/rigger-grounder/tests/code_view_golden.rs` naming its rule;
  `code_view.golden` is unchanged.
- Read budgets, measured by `tests/read_budgets.rs` on the release binary: `rigger graph
  --around` and `--show` under 50 ms; `rigger graph --lookup` under 250 ms; the dash `/api/graph`
  overview under 150 ms by default, under 300 ms with `tests=include` and under 300 ms at a
  non-default resolution.
- Each unit whose added or dropped mentions change a pinned golden re-pins it on its own tree.
- Hyphens, not em dashes. Both feature lanes green.

## Done when

- [ ] a test in `crates/rigger-domain/src/resolve.rs` proves A QUALIFIED CALL NARROWS ITS CANDIDATES: over hand-built facts, `FactsView::new` written in another file resolves EXTRACTED to `FactsView`'s `new`.
  This criterion OWNS the narrowing in `call` and `tagged`.
- [ ] a test in `crates/rigger-domain/src/resolve.rs` proves SEVERAL NARROWED CANDIDATES NEVER PREFER THEIR FILE: `CodeView.file` written in a trait default method of `CodeView`, with two implementations one of which shares the file, goes to the placeholder keyed by the qualified name, whose down-hop is both, and `Composed`'s call walk lists both as its callees and the caller under each.
  This criterion OWNS the qualified placeholder key, its down-hop and every placeholder reader outside `resolve.rs`.
- [ ] a test in `crates/rigger-domain/src/resolve.rs` proves A QUALIFIER NAMING NO DEFINITION: `AtomicU64::new` beside a project `Cursor::new` makes no link and no unresolved entry, and `Request.GetClientIp` resolves by the unqualified rule.
  This criterion OWNS the path and member fallbacks.
- [ ] a test in `crates/rigger-domain/src/resolve.rs` proves A QUALIFIED PROOF RESETTLES: adding in another file a second `impl Cursor` holding `new` moves the proof of a test's `Cursor::new()` from the first `new` to none, and `resettle`'s settled bytes equal a full settle's there, after a differing file that only adds `struct AtomicU64` beside a test's `AtomicU64::new()`, and after one that only renames an `impl` mention over unchanged method ids.
  This criterion OWNS the proof narrowing, the `resettle` dirty set and the `SETTLED_GENERATION` move.
- [ ] a test in `crates/rigger-domain/src/resolve.rs` proves THE NEW TAGS RESOLVE AS REFERENCES: hand-built `variant` and `impl` mentions resolve to a variant and a trait, each a REFERENCES link and never CALLS.
  This criterion OWNS `REF_KIND_FIELD`, `REF_KIND_VARIANT`, `REF_KIND_IMPL`, their candidate kinds and the `EdgeInferred` ref-kind doc comment.
- [ ] a test in `crates/rigger-domain/src/resolve.rs` proves AN UNQUALIFIED FIELD USE NEVER PREFERS ITS FILE: `x.lang` beside one same-file `lang` field and one in another file gives no edge, and beside one `lang` field in another file gives an INFERRED edge.
  This criterion OWNS the unqualified field rule.
- [ ] a test in `crates/rigger-domain/src/near.rs` proves NEAR FOLDS QUALIFIED CALLERS: `--around FactsView::new` over a near view lists the cross-file `FactsView::new` caller the whole view draws.
  This criterion OWNS the `State::landing` fold and its module doc.
- [ ] a test in `crates/rigger-store/src/code/snapshot.rs` proves DOC SPANS AND MODULE LISTS SURVIVE A SNAPSHOT: facts carrying `Def.doc`, `FileFacts.doc`, an absent span and a `ModuleDecl` with distinct `as_root` and `as_child` lists and `path_attr` read back equal at `VERSION` 5.
  This criterion OWNS the facts fields in `code_facts.rs`, the snapshot rows and `VERSION`.
- [ ] a test in `crates/rigger-store/src/code/advance.rs` proves A VERSION-4 STORE REBUILDS: a store holding a version-4 snapshot opens as absent and its next advance writes a version-5 snapshot from empty.
  This criterion OWNS that test only.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves RUST DOC SPANS ARE EXTRACTED: a definition under `///` lines with `#[doc(hidden)]` between them gets the span from its first to its last doc line, a file opening with `//!` lines gets its file span, and a fixture query's `@doc` capture gives its definition the captured span.
  This criterion OWNS the Rust doc rule, the `@doc` read in `tags_in`, the `model::Def` and `FileSymbols` spans and their copy in `code_facts`.
- [ ] a test in `crates/rigger-grounder/src/facts.rs` proves THE DOC TEXT IS STRIPPED: `FactsExtractor::doc_text` turns a Rust span's raw lines, `\r\n` endings and a `#[doc(hidden)]` line included, into the comment text alone.
  This criterion OWNS `Extract::doc_text`.
- [ ] a test in `crates/rigger-domain/src/resolve.rs` proves A MODULE ROOT IS DECIDED AT READ: `module_target` reaches `src/bin/x.rs` from `mod x;` in `src/bin/tool.rs`, `p/z.rs` from `mod z;` in a file `src/lib.rs` loads by `#[path = "p/y.rs"]`, and `src/code/advance.rs` from `mod advance;` in a reached `src/code/mod.rs`.
  This criterion OWNS `module_target`'s list choice and reach rule.
- [ ] the table `module_candidates_name_the_candidates_of_each_tabled_declaration_in_order` in `crates/rigger-grounder/src/grounder/symbols/paths.rs` proves EACH DECLARATION HAS BOTH LISTS: each row pins an (`as_root`, `as_child`) pair, with rows for a bin root's `mod x;`, a `#[path]`-loaded file's `mod z;` and `mod a { #[path = "b.rs"] mod b; }` in `src/m.rs`.
  This criterion OWNS `module_dir`, `module_candidates` and `path_attr` extraction.
- [ ] the registry golden in `crates/rigger-grounder/src/grounder/symbols/registry.rs` and the facts golden in `crates/rigger-grounder/src/facts.rs` prove EACH LANGUAGE PINS ITS OWN ROWS: each pins one (language, tags generation, digest) row per language, the facts golden one (`FACTS_GENERATION`, digest) row besides, and `GRAMMAR_TAGS_VERSION`'s `concat!` output equals the table's consts joined in table order.
  This criterion OWNS the generation macros and consts, the table and both goldens' rows.
- [ ] a test in `crates/rigger-grounder/src/facts.rs` proves ONE TAGGED MENTION PER SITE KIND: a function reading one constant three times yields one `Tagged("const")` mention at the first line, and three calls to one function yield three call mentions.
  This criterion OWNS the dedupe in `FactsExtractor`.
- [ ] a test in `crates/rigger-grounder/src/facts.rs` proves EVERY CAPTURE KEEPS ITS SUFFIX: a fixture query's `@reference.class`, `@reference.implementation`, `@reference.constant`, `@reference.field` and `@reference.import` captures yield `type`, `impl`, `const`, `field` and `use` mentions, and `@reference.call` a call.
  This criterion OWNS the change to `Capture::Reference` in `compile`.
- [ ] a test in `crates/rigger-grounder/src/facts.rs` proves A CONSTRUCTOR STAYS A CALL: a TypeScript, a TSX, a JavaScript and a C# `new Square()` each yield a call mention.
  This criterion OWNS the `constructors` field and its four entries.
- [ ] a test in `crates/rigger-grounder/src/facts.rs` proves A QUALIFIER CAPTURE NAMES THE MENTION: a fixture query's `@qualifier.path` and `@qualifier.member` yield mentions named `Shape::area` and `Shape.area` whose `leaf` is `area`, and a `@name` over `Ns.IShape` and over `List<IShape>` yields leaves `IShape` and `List`.
  This criterion OWNS `SymRef.qualifier`, `Qualifier`, the leaf split, the `type_arguments` field and the facts layer's name write.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/grounder.rs` proves AN IMPLEMENTING FILE STAYS IN THE BLAST RADIUS: under a fixture tags query, a Python file whose class names `Shape` as its base, and nothing else of `Shape`, is among `Shape`'s radius referencer files, and `Shape`'s reference degree counts no `impl` mention.
  This criterion OWNS `Symbols::radius`'s read of `impl` mentions.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves MACRO TOKEN TREES ARE READ: a call inside `vec![..]`, a constant inside `assert_eq!(..)`, a call inside `json!({"k": f(x)})` and a `static` inside `thread_local!` yield a call, a const, a call and a definition at their lines, and a `macro_rules!` body yields nothing.
  This criterion OWNS the token-tree re-parse.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves RUST WRITES THE QUALIFIER: `Store::open()`, `Self::new()` inside `impl<T> Store<T>` and `self.file()` inside `trait CodeView` yield calls named `Store::open`, `Store::new` and `CodeView.file`, and `self.inner.open()` and `paths::module_dir()` yield no qualifier.
  This criterion OWNS the Rust qualifier rule.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves AN EXCLUDED QUALIFIER STAGES NOTHING: with `use std::io::Cursor;`, `Cursor::new(x)` and `assert_eq!(c, Cursor::new(v))` yield no mention, and with `use a::B as C;`, `C::f()` yields a call named `B::f`.
  This criterion OWNS the qualifier's pass through `excluded` and the alias rule.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves RUST STAGES FUNCTION VALUES: `.map(entity_block)` and `.map(Store::open)` yield calls named `entity_block` and `Store::open`, and `.map(x)` with `x` a parameter and `Err(e) => Err(e)` yield none.
  This criterion OWNS the function-value rule.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves RUST STAGES FIELD USES: `opts.system_prompt = x`, `SpawnOpts { system_prompt }` and `self.root` inside `impl Store` yield `field` references named `system_prompt`, `SpawnOpts::system_prompt` and `Store.root`, and `opts.run()` yields none.
  This criterion OWNS the Rust field rule.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves RUST STAGES VARIANT USES: `InstallOutcome::AlreadyCurrent` in an expression and `Located::Many(c) =>` in a pattern yield `variant` references, `Kind::RejectRecurrence { n } =>` yields a `variant` mention named `Kind::RejectRecurrence` and a `field` mention named `RejectRecurrence::n` and no `type` mention, `Hop::Frontier(cs)` stays a call, and `-> Self::Target` yields none.
  This criterion OWNS the Rust variant rule.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves RUST TAGS IMPL HEADERS: `impl<T> CodeView for FactsView<T>` yields `impl` mentions of `CodeView` and `FactsView` enclosed by the impl, and `impl FactsView` one of `FactsView`, with no call and no `type` mention.
  This criterion OWNS the impl-header rule in `extract`, `block.targets` and `impl_from`.
- [ ] a test in `crates/rigger-grounder/src/grounder/symbols/extract.rs` proves RUST READS PATTERN AND FORMAT CONSTANTS: `REF_KIND_TYPE =>`, `format!("{TEMP}")` and `assert_eq!(a, b, "{LIMIT}")` yield `const` references of `REF_KIND_TYPE`, `TEMP` and `LIMIT`, and `format!("{temp}")`, `format!("{{TEMP}}")` and `assert_eq!(out, "{TEMP}")` yield none.
  This criterion OWNS the const-read and format-capture rules.
- [ ] a table test in `crates/rigger-grounder/tests/facts_python.rs` proves PYTHON STAGES ITS ROW: a constant read, an annotation, a base class, `x.f`, `Shape.area()` and a docstring yield `const`, `type`, `impl` and `field` mentions, a call named `Shape.area` and a doc span.
  This criterion OWNS `PYTHON_TAGS` and Python's doc rule.
- [ ] a table test in `crates/rigger-grounder/tests/facts_go.rs` proves GO STAGES ITS ROW: a parameter type yields a `type` mention and no call, `x.f` not called yields a `field` mention, and a `//` block above a function yields its doc span.
  This criterion OWNS `GO_TAGS`.
- [ ] a table test in `crates/rigger-grounder/tests/facts_typescript_javascript.rs` proves TYPESCRIPT AND JAVASCRIPT STAGE THEIR ROWS: in TypeScript `IShape[]`, `implements IShape` and a constant read yield `type`, `impl` and `const` mentions; in JavaScript `extends Base`, `x.f` and `Square.make()` yield an `impl` mention, a `field` mention and a call named `Square.make`; a `/** */` block yields a doc span in both.
  This criterion OWNS the TypeScript and JavaScript registry entries' queries.
- [ ] a table test in `crates/rigger-grounder/tests/facts_csharp.rs` proves C# STAGES ITS ROW: `class Square : IShape`, `void Draw(List<IShape> s)`, `Square.SIDES`, `s.Side = 2` and a `///` block yield one `impl` mention, a `type` mention, a `field` mention named `Square.SIDES`, a `field` mention named `Side` and a doc span read back without its XML tags.
  This criterion OWNS `CSHARP_TAGS` and C#'s doc rule.
- [ ] a test in `tests/graph_honesty_label_cli.rs` proves THE LOOKUP NAMES REFERRERS BY KIND: through the binary, a documented constant read by two functions prints a `reads:` line naming both and the doc's first line under its site line, and a type named in a signature prints a `type use:` line naming that function.
  This criterion OWNS `Resolver::referrers`, `ContextGraph::referrers`, `answer`'s fill and `entity_block`.
- [ ] a table test in `crates/rigger-grounder/src/lookup/tests.rs` proves EVERY TAG HAS ITS LABEL: one row per tag in the label table prints its label, and a row with more referrers than the cap prints the `(+N more)` tail.
  This criterion OWNS the label table.
- [ ] a test in `tests/graph_honesty_label_cli.rs` proves A TEST REFERRER IS HIDDEN BY DEFAULT: a constant read by a product function and a test function lists the test function only under `--tests include`.
  This criterion OWNS the tests filter on `ContextGraph::referrers`.
- [ ] a test in `tests/graph_honesty_label_cli.rs` proves AN AMBIGUOUS REFERRER IS MARKED: a constant `LIMIT` defined in two files and read in a third lists the reading function with `?` under each constant's `reads:` line.
  This criterion OWNS the `?` mark.
- [ ] a test in `tests/graph_honesty_label_cli.rs` proves SHOW PRINTS THE DOC AND THE REFERRERS: `rigger graph --show` of a documented constant prints its stripped doc and a `referenced by reads` entry, and of a file opening with `//!` lines prints the file's doc.
  This criterion OWNS `ContextGraph::doc`, `SourceLines`, the `EntitySite` fields and their `site_fields` entries.
- [ ] a test in `crates/rigger-dash/src/mcpserver.rs` proves THE MCP SHOW CARRIES THE DOC AND THE REFERRERS: the `rigger_graph` tool's `show` of a documented constant answers `doc_comment` and `referenced by reads` keys.
  This criterion OWNS that test only.
- [ ] a test in `tests/graph_honesty_label_cli.rs` proves THE OVERLAY CARRIES NEW REFERENCES: an uncommitted edit adding a constant read in one worktree shows that referrer in a lookup from that worktree and not in a lookup from a second worktree at the same head.
  This criterion OWNS that test only.
- [ ] `tests/read_budgets.rs`, run with `RIGGER_READ_BUDGETS=1`, proves THE READ BUDGETS HOLD: it clones HEAD into `.rigger/tmp/agent-scratch`, builds the release binary once, runs `rigger graph build`, starts the dash on an ephemeral port, and measures each read of the Global constraints' budgets as the median of five runs after one warm-up; a drop guard removes the clone and stops the dash it started on every exit path.
  This criterion OWNS the test, its `[[test]]` entry in `Cargo.toml` and any read-path change a missed budget requires; the gates do not run it, so the unit's evidence carries the measured figures.

## Notes

**Units.** Seven units; the planner keeps these lines.

| Unit | Criteria | Needs | Estimated added lines |
|---|---|---|---|
| resolver | 1 to 7 | none | 2,300 |
| facts format | 8 to 19 | resolver | 2,800 |
| Rust references | 20 to 27 | facts format | 2,850 |
| Python and Go | 28, 29 | facts format | 1,000 |
| TypeScript, JavaScript and C# | 30, 31 | facts format | 1,300 |
| readers | 32 to 38 | facts format | 1,900 |
| budgets | 39 | Rust references, Python and Go, TypeScript JavaScript and C#, readers | 400 |

The longest chain is four units: resolver, facts format, Rust references, budgets. One unit
starts at once; four run side by side once facts format lands. Facts format needs resolver
because both move `SETTLED_GENERATION` and its pinned digest (criteria 4 and 12). The language
units and readers use only facts format's captures, fields and qualifier write; a language's
query row and its doc rule are one concern per non-Rust language.

**Landing order.** Each extractor unit moves only its language's generation and rows (criterion
14); the rows sit one per line, so a merge that meets two units' adjacent rows keeps both. Each
language's table test is a file of its own, so the parallel extractor units share no test file.
Readers' fixtures read constants and type uses, which stage before Rust references lands.

**Goal figures.** The two Goal figures are the next evaluation's to read and no unit's to prove.

**Mention growth.** At 26942c67 the snapshot holds 43,174 product mentions (30,711 calls, 12,463
tagged; 32,395 distinct per enclosing, name and kind) against the 34,301 the budgets were set at.
Production sources hold about 18,500 member-access sites against about 102,000 call sites;
members, macro arguments, function values and variants are estimated to add 15,000 to 25,000
product mentions after the dedupe. At 26942c67 the installed binary answers `--around` in about
20 ms and `--lookup` in 200 to 240 ms, the text and prose scans included; referrers read one
leaf's postings, so their cost does not grow with the store.

**Evidence to criteria.** The class's five lines: constant reads (criteria 20, 27, 32), field
assignments (criteria 24, 32), a variant match (criteria 25, 32); `progress:59611` names
`ErrorKind::NotFound`, a standard-library variant, which no criterion reaches. The caller target's
lines move with criteria 1, 7, 20, 21 and 23 (calls through paths, macro arguments and function
values).

**Observed at 26942c67.**

| Kind | Answered | Evidence |
|---|---|---|
| Const read in an expression | partly | `rust_references` REFERENCES `REF_KIND_TYPE`; `GREP_GUARD_MESSAGE` referenced in `guard.rs`; `attention_kind_rank::ORDER` referenced in `ledger.rs`; a read inside `vec![CODE_KIND_TYPE, CODE_KIND_TRAIT]` or a match arm `REF_KIND_TYPE =>` gives none |
| Static read | partly | `GO_TAGS` referenced from `registry.rs`; `format.rs::TEMP_SEQ`, read only inside `format!`, has none; the `thread_local!` static `CONSOLE` is no definition |
| Type in a signature, field or body | yes | `MentionKind`, `ModuleDecl`, `ReviewRound` referenced from fns, fields and files |
| Trait implemented | wrong rel | `impl CodeView for FactsView` CALLS `CodeView`; `CodeView` called by `impl Runner for ExecRunner`; `SpawnOpts` called by `impl SpawnOpts` |
| Re-export | as a plain import | `conductor`, `META_REPLAY_KEY` and `TYPE_BLAST_RADIUS_COMPUTED` are referenced in `conductor.rs` by its `pub use`, as by a `use` |
| `mod` declaration to its file | partly | `lib.rs::code_facts` CONTAINS `code_facts.rs`; `build.rs` reaches `build/gitsemver.rs`; a bin root's `mod x;`, a `#[path]`-loaded file's children and an inline module's `#[path]` miss |
| Macro use | yes, as a call | `agent.rs` CALLS `error_from_display` |
| Call or read inside macro arguments | no | `code_view.rs::tests::def`, called three times inside `vec![]`; `site_object`, called three times inside `json!({..})`; `TEMP_SEQ` inside `format!` |
| Function passed by name | no | `entity_block` (passed to `map` in `budget.rs`) and `base_type_identifier` (passed to `filter_map` in `extract.rs`) have no caller |
| Path or receiver call | wrong edges | `TEMP_SEQ` CALLS `Cursor::new` (written `AtomicU64::new`); `CodeView::modules` CALLS `<FactsView as CodeView>::file` alone (written `self.file()`); `FactsView::new` CALLS itself |
| Field read or assignment | no | `system_prompt`, `Entity.referenced_in` and the four `tests_hidden` fields are nodes with no referencing site |
| Enum variant use | partly | a tuple-variant construction is a call (`Hop::Frontier` called by `hop`, `Located::Many` called by `locate`); the pattern `Located::Many(cands) =>` in `mcpserver.rs` and the unit variant `InstallOutcome::AlreadyCurrent` reach the enum or nothing |
| Doc comment | no | `Def` and `FileFacts` hold no doc; `--show REF_KIND_TYPE` prints none |
| Python | calls only | a class constructor call; no constant read, annotation or base class |
| Go | wrong rel | the type use in `Build` CALLS `Shape`; no doc |
| TypeScript, JavaScript | wrong rel | an interface use is a call; the constant `SIDES` has no referrer; no doc |
| C# | wrong rel | `class Square : IShape` CALLS `IShape`; `Square.SIDES` has no referrer; no doc |

A `pub use` stays an import, listed as `imported by`: a path through it resolves by its leaf, so
no re-export node is added.

**Issues.**

| Issue | Disposition | Reason |
|---|---|---|
| #120 | criteria 2, 21 | a trait default method's `self.file()` still lands on the same-file implementation |
| #115 | criterion 20 | macro token trees stage nothing today |
| #110 | criteria 12, 13 | the three shapes miss today |
| #43 | criteria 6, 24, 32 | field definitions are nodes, their uses are not |
| #40 | fixed | the per-commit snapshot answers `--show src/conductor.rs::critique_gate_name` with "no such entity" |
| #34 | fixed | `git_commit_all` is a test node with its callers, hidden by default |
| #70 | fixed | `settle_node` no longer exists |
| #42 | fixed by the overlay; criterion 38 covers the new facts | an uncommitted helper is found from its worktree today |
| #38 | out of scope | a call-by-argument query is no reference kind; the lookup's text scan answers it |
