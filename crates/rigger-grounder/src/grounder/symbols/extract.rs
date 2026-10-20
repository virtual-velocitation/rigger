//! Symbol extraction: the ONE function that touches tree-sitter (architecture 5.5.3). It
//! runs an INJECTED grammar's `tags` query over a source string and lowers the uniform tags
//! into the parser-free `FileSymbols` model. Everything downstream sees `FileSymbols` and
//! never the parser - so per-language support is a registration (unit 2), and a language
//! whose query cannot tag a definition (its grammar gives it no name node, or the upstream query
//! already holds that node as a reference) also adds a read-off here, selected by the language
//! `extract` is given.

use crate::grounder::symbols::events::kind_str;
use crate::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef};
use rigger_domain::wave::normalize_ws;
use tree_sitter_tags::{TagsConfiguration, TagsContext};

/// Map a grammar tag's syntax-type NAME (the name half of a `tags.scm` category, e.g.
/// "function", "method", "struct", "class", "interface", "trait", "module", "constant", "macro",
/// "static", "variant", "field", "variable", "constructor", "property") to a rigger `Kind`. Unknown names fold to `Other`,
/// so the model stays grammar-agnostic and a new grammar never forces a `Kind` variant.
fn kind_of(syntax_type: &str) -> Kind {
    match syntax_type {
        "function" => Kind::Function,
        "method" => Kind::Method,
        "struct" | "class" | "enum" | "type" | "interface" => Kind::Type,
        "trait" => Kind::Trait,
        "impl" => Kind::Impl,
        "module" | "namespace" => Kind::Module,
        "constant" => Kind::Constant,
        "macro" => Kind::Macro,
        "static" => Kind::Static,
        "variant" => Kind::Variant,
        "field" => Kind::Field,
        "variable" => Kind::Variable,
        "constructor" => Kind::Constructor,
        "property" => Kind::Property,
        _ => Kind::Other,
    }
}

/// THE NAME FILTER (spec 116): whether a definition whose leaf is `leaf` names nothing in `lang`.
/// The empty leaf names nothing in any language. `_` names nothing in Go and Rust, whose
/// grammars make it the blank identifier, which nothing can refer to; in JS, TS, Python and C#
/// `_` is an ordinary name its `_(..)` calls resolve to.
fn is_nameless(lang: Lang, leaf: &str) -> bool {
    leaf.is_empty() || (leaf == "_" && matches!(lang, Lang::Go | Lang::Rust))
}

/// A DECLARATION (spec 116): whether `node`, the own node of a function, method or constructor
/// definition, has no body. Rust's is a `function_signature_item`, a trait required fn or an
/// extern block's fn; Go's is an interface's `method_elem`, and a `function_declaration` or a
/// `method_declaration` holding no `body`; C#'s is a `method_declaration`, a
/// `destructor_declaration`, a `local_function_statement`, a `constructor_declaration`, an
/// `operator_declaration` or a `conversion_operator_declaration` holding no `body` (an interface,
/// `abstract`, `extern` or `partial` member); TS's is a `function_signature` (an ambient or
/// overload function), a `method_signature` (an interface's method) or an
/// `abstract_method_signature`. It reads the node alone, so a grammar sharing one
/// of those node types shares its rule.
fn is_declaration(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "function_signature_item"
        | "method_elem"
        | "function_signature"
        | "method_signature"
        | "abstract_method_signature" => true,
        "function_declaration"
        | "method_declaration"
        | "destructor_declaration"
        | "local_function_statement"
        | "constructor_declaration"
        | "operator_declaration"
        | "conversion_operator_declaration" => node.child_by_field_name("body").is_none(),
        _ => false,
    }
}

/// A FUNCTION-VALUED FIELD (spec 116): whether `node`, the own node of a field definition, holds
/// an arrow function or a function expression as its `value`, a JS `field_definition` or a TS
/// `public_field_definition` (`act = () => {}`), which is then a method. It reads the node alone,
/// as [`is_declaration`] does, those two value types being JS's and TS's alone.
fn is_function_valued(node: tree_sitter::Node) -> bool {
    node.child_by_field_name("value")
        .is_some_and(|v| matches!(v.kind(), "arrow_function" | "function_expression"))
}

/// A PYTHON METHOD (spec 116): whether `node`, the own node of a definition, is a Python
/// `function_definition` whose nearest enclosing definition is a `class_definition`, the
/// statements, blocks and `decorated_definition` wrappers between them passed through; a def
/// nested in a function, a method's included, is a function. It reads the node and its
/// ancestors alone, as [`is_declaration`] reads the node.
fn is_python_method(node: tree_sitter::Node) -> bool {
    node.kind() == "function_definition"
        && std::iter::successors(node.parent(), |n| n.parent())
            .find(|n| matches!(n.kind(), "class_definition" | "function_definition"))
            .is_some_and(|n| n.kind() == "class_definition")
}

/// THE LEAF of a tagged definition (spec 116): the name its `Def` and its caller range carry,
/// read over `node`, its own node, and `name`, the slice of the source its tag names. A C#
/// `destructor_declaration` is `~` and that identifier (`~Foo`), the grammar holding the `~`
/// outside the name node; a JS or TS definition named by a `string` (`declare module "x"`, a
/// member `"a-b"`), or the `string` node itself, is the text between its quotes
/// ([`unquoted`]); every other definition's leaf is the slice itself, a private member's `#`
/// and a numeric member's digits included. It reads the node alone, as [`is_declaration`]
/// does.
fn def_leaf(node: Option<tree_sitter::Node>, name: &str) -> String {
    match node {
        Some(n) if n.kind() == "destructor_declaration" => format!("~{name}"),
        Some(n) if n.child_by_field_name("name").unwrap_or(n).kind() == "string" => {
            unquoted(name).to_string()
        }
        _ => name.to_string(),
    }
}

/// The source text between the quotes of `text`, a JS or TS string's slice, escapes undecoded:
/// its first and last character dropped, a quote each; `text` itself when it is shorter than
/// two characters.
fn unquoted(text: &str) -> &str {
    text.get(1..text.len().saturating_sub(1)).unwrap_or(text)
}

/// Whether `node` is a Rust fn the compiler registers as a procedural macro: a `function_item`
/// whose outer attribute stack holds `#[proc_macro]`, `#[proc_macro_derive(..)]` or
/// `#[proc_macro_attribute]`, wherever in the stack it sits.
fn is_proc_macro_fn(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.kind() == "function_item"
        && scan_attribute_stack(
            source,
            node.prev_sibling(),
            |n| n.prev_sibling(),
            |item, s| {
                let (_, name) = named_attribute(item, s)?;
                matches!(
                    name,
                    "proc_macro" | "proc_macro_derive" | "proc_macro_attribute"
                )
                .then_some(())
            },
        )
        .is_some()
}

/// The kind a definition the query tagged `tagged` lowers to, read over `node`, its own node
/// (the deepest one spanning its range): a function, method or constructor with no body
/// ([`is_declaration`]) is a `Declaration`, a function-valued field ([`is_function_valued`]) a
/// `Method`, a Python def in a class ([`is_python_method`]) a `Method`, and in Rust a proc-macro
/// fn ([`is_proc_macro_fn`]) is a `Macro`; every other definition keeps the kind its tag names.
fn lowered_kind(lang: Lang, tagged: Kind, node: Option<tree_sitter::Node>, source: &[u8]) -> Kind {
    match tagged {
        Kind::Function | Kind::Method | Kind::Constructor if node.is_some_and(is_declaration) => {
            Kind::Declaration
        }
        Kind::Field if node.is_some_and(is_function_valued) => Kind::Method,
        Kind::Function if node.is_some_and(is_python_method) => Kind::Method,
        _ if lang == Lang::Rust && node.is_some_and(|n| is_proc_macro_fn(n, source)) => Kind::Macro,
        _ => tagged,
    }
}

/// One file's source text with the byte offset each of its lines begins at, computed once per
/// [`extract`] so [`def_of`] maps a definition's last byte to its line by one binary search
/// rather than rescanning the text per definition.
struct Source<'s> {
    text: &'s str,
    line_starts: Vec<usize>,
}

impl<'s> Source<'s> {
    fn new(text: &'s str) -> Self {
        Source {
            text,
            line_starts: line_start_offsets(text),
        }
    }
}

/// THE ONE CONSTRUCTOR of a `Def` (spec 119, ONE EXTENT): every tagged, read-off and impl
/// definition [`extract`] records, in every language. `line` is the 1-based line its pass gives
/// it (a tagged definition's name row, a read-off's as its reader gives it) and `end_line` the
/// 1-based line of `range`'s last byte, `range` the definition's whole byte range, never below
/// `line`; a zero-length `range` gives `line`. Its `path` is its `name` here and it has no
/// `parent`. It is built ahead of the passes over the whole file that resolve the rest: `path`
/// and `parent` need every definition's range ([`def_paths`] decides containment over the whole
/// set), as `is_test` does (`test_regions`),
/// and the out-of-line module facts are read from the parsed tree once every definition is known
/// (only a `Module`-kind definition ever carries them; see `Def::is_out_of_line_module`).
fn def_of(
    source: &Source,
    range: std::ops::Range<usize>,
    kind: Kind,
    name: String,
    line: u32,
) -> Def {
    let end_line = if range.is_empty() {
        line
    } else {
        (line_of_byte(&source.line_starts, range.end - 1) + 1).max(line)
    };
    Def {
        kind,
        path: name.clone(),
        parent: None,
        name,
        line,
        end_line,
        is_test: false,
        is_out_of_line_module: false,
        path_override: None,
        enclosing_inline_module_path: None,
    }
}

/// Record one definition the tag pass or a read-off found, `tagged` the kind it names and
/// `node` its own node, into `defs` and `def_ranges` at one index. THE NAME FILTER runs first: a
/// definition whose leaf names nothing in `lang` ([`is_nameless`]) enters neither, so its range
/// is no caller range, and is returned instead, for [`test_regions`] to read when it is test
/// attributed; every other takes its kind lowered over its node ([`lowered_kind`]) and returns
/// `None`.
fn record_def(
    lang: Lang,
    source: &Source,
    node: Option<tree_sitter::Node>,
    tagged: Kind,
    (range, name, line): ReadOff,
    defs: &mut Vec<Def>,
    def_ranges: &mut Vec<(std::ops::Range<usize>, String, Kind)>,
) -> Option<std::ops::Range<usize>> {
    if is_nameless(lang, &name) {
        return Some(range);
    }
    let kind = lowered_kind(lang, tagged, node, source.text.as_bytes());
    defs.push(def_of(source, range.clone(), kind, name.clone(), line));
    def_ranges.push((range, name, kind));
    None
}

/// Extract one file's definitions and references by running the INJECTED grammar's tag query
/// over `source` (5.5.3). This is the ONLY function that touches tree-sitter; the caller
/// supplies the grammar and its query (unit 2's registry decides WHICH), and everything it
/// returns is the parser-free `FileSymbols`.
///
/// A tagged definition is named by its leaf ([`def_leaf`]), in `defs` and in its caller range
/// alike, and as a caller by its path (`Def.path`, spec 119 criterion 8). A definition tag
/// whose leaf names nothing ([`is_nameless`]) makes no `Def`, and the references in its body
/// take the next enclosing definition by the caller rule, else the file. A
/// definition read off the tree by `lang` is put to that filter too, so a Go `type _ = B` makes
/// none. An impl block is not: its name is its header, `impl ..`, never empty and never `_`.
///
/// A tagged definition takes the kind its tag names, lowered once over its own node
/// ([`lowered_kind`]). What the query cannot tag is read off the parsed tree: every impl block
/// ([`impl_blocks`]) and, selected by `lang`, Rust's tuple fields ([`tuple_fields`]), Go's
/// embedded fields and type aliases ([`embedded_fields`], [`named_by_field`]) and C#'s operators,
/// conversion operators and indexers ([`named_by_field`], [`conversion_operators`],
/// [`indexers`]), JS's and TS's class fields, valueless enum members and the identifiers a
/// pattern declarator or a declaring `for .. in`/`for .. of` binds ([`class_fields`],
/// [`enum_members`], [`js_bindings`]), and Python's module-level names a pattern or a chain
/// binds, class-body names and type aliases ([`assigned_names`], [`class_body_names`],
/// [`type_aliases`]), each recorded as a tagged definition is ([`record_def`]). Every definition,
/// an impl block's included, is built by [`def_of`] from its byte range, which gives its
/// `end_line`, and takes its id path and parent from [`def_paths`].
///
/// A tag whose byte range cannot be sliced out of `source` (a defensive guard; the ranges the
/// tags mechanism yields are valid UTF-8 boundaries in practice) is skipped rather than
/// panicking, so a pathological file degrades to partial symbols, never a crash.
pub fn extract(
    source: &str,
    lang: Lang,
    ts_language: &tree_sitter::Language,
    tags_query: &str,
) -> Result<FileSymbols, String> {
    let config = TagsConfiguration::new(ts_language.clone(), tags_query, "")
        .map_err(|e| format!("symbols: tags config: {e}"))?;
    let mut ctx = TagsContext::new();
    let (tags, _) = ctx
        .generate_tags(&config, source.as_bytes(), None)
        .map_err(|e| format!("symbols: generate tags: {e}"))?;
    // The full parse of `source` (the SAME tree `tags_query` runs against, parsed a second time with
    // the full grammar), read for what the tags pass cannot express: a tagged definition's own
    // node, impl blocks, tuple fields, test attributes, and out-of-line module declarations. This
    // is still the one function touching tree-sitter, so the single-parsing-authority invariant
    // (5.5.3) survives. A `Parser::set_language` or `parse` failure here is defensive only:
    // `ts_language` already configured the tags query above, using the identical language, so
    // this path degrades to an `Err` rather than a panic without ever being expected to fire in
    // practice.
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(ts_language)
        .map_err(|e| format!("symbols: parser language: {e}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "symbols: parse: tree-sitter produced no syntax tree".to_string())?;
    let root = tree.root_node();
    let text = Source::new(source);
    let mut defs = Vec::new();
    let mut refs = Vec::new();
    // Each definition's byte range (the whole construct, body included - the tag's node range,
    // see `enclosing_def`) with its name and kind, and each reference's byte position. Collected
    // in this single tag pass; the enclosing definition is resolved below so the reference order
    // the emit pass depends on stays untouched.
    let mut def_ranges: Vec<(std::ops::Range<usize>, String, Kind)> = Vec::new();
    // The range of each definition THE NAME FILTER drops (`record_def`), in neither `defs` nor
    // `def_ranges`, kept for `test_regions` alone.
    let mut dropped: Vec<std::ops::Range<usize>> = Vec::new();
    let mut ref_positions: Vec<usize> = Vec::new();
    for tag in tags {
        let tag = tag.map_err(|e| format!("symbols: tag: {e}"))?;
        let Some(name) = source.get(tag.name_range.start..tag.name_range.end) else {
            continue;
        };
        // 1-based line from the tag span's start row.
        let line = tag.span.start.row as u32 + 1;
        if tag.is_definition {
            // THE LEAF, ahead of the filter and both pushes `record_def` makes, so the name a
            // definition is filtered by, recorded under and ranked by as a caller is one.
            let node = root.descendant_for_byte_range(tag.range.start, tag.range.end);
            dropped.extend(record_def(
                lang,
                &text,
                node,
                kind_of(config.syntax_type_name(tag.syntax_type_id)),
                (tag.range.clone(), def_leaf(node, name), line),
                &mut defs,
                &mut def_ranges,
            ));
        } else {
            ref_positions.push(tag.range.start);
            refs.push(SymRef {
                name: name.to_string(),
                line,
                enclosing: None,
                is_test: false,
            });
        }
    }
    // Every impl block is a definition of its own (gap 104), named by its header, with its trait
    // and self type recorded as references. Added before the attribution pass below, so those two
    // references attribute to the impl (the impl -> trait and impl -> type links) unless a
    // function encloses the impl, which then is their caller, while a reference inside one of its
    // methods still attributes to the method, the innermost function.
    for block in impl_blocks(root, source.as_bytes()) {
        defs.push(def_of(
            &text,
            block.range.clone(),
            Kind::Impl,
            block.name.clone(),
            block.line,
        ));
        def_ranges.push((block.range, block.name, Kind::Impl));
        for (name, line, pos) in block.targets {
            ref_positions.push(pos);
            refs.push(SymRef {
                name,
                line,
                enclosing: None,
                is_test: false,
            });
        }
    }
    // The definitions the language's query cannot tag are read off the tree too (spec 116), each
    // read-off giving the kind of what it finds, lowered over its own node as a tagged
    // definition's is; one whose leaf names nothing makes no definition, as a tagged one makes
    // none.
    let read_offs = match lang {
        Lang::Rust => vec![(Kind::Field, tuple_fields(root))],
        Lang::Go => vec![
            (Kind::Field, embedded_fields(root, source.as_bytes())),
            (
                Kind::Type,
                named_by_field(root, source.as_bytes(), "type_alias", "name"),
            ),
        ],
        Lang::CSharp => vec![
            (
                Kind::Method,
                named_by_field(root, source.as_bytes(), "operator_declaration", "operator"),
            ),
            (Kind::Method, conversion_operators(root, source.as_bytes())),
            (Kind::Method, indexers(root, source.as_bytes())),
        ],
        Lang::Js | Lang::Ts => vec![
            (Kind::Field, class_fields(root, source.as_bytes())),
            (Kind::Variant, enum_members(root, source.as_bytes())),
            (Kind::Constant, js_bindings(root, source.as_bytes())),
        ],
        Lang::Python => vec![
            (
                Kind::Constant,
                assigned_names(root, source.as_bytes(), true),
            ),
            (Kind::Field, class_body_names(root, source.as_bytes())),
            (Kind::Type, type_aliases(root, source.as_bytes())),
        ],
    };
    for (kind, found) in read_offs {
        for found in found {
            let node = root.descendant_for_byte_range(found.0.start, found.0.end);
            dropped.extend(record_def(
                lang,
                &text,
                node,
                kind,
                found,
                &mut defs,
                &mut def_ranges,
            ));
        }
    }
    // Spec 119 (IDS FOLLOW THE LANGUAGE'S PATH; CONTAINS IS STRUCTURE): each definition's id path
    // and parent, decided over every definition's range at once, so a container recorded after
    // what it holds still prefixes and parents it.
    for (d, (path, parent)) in defs
        .iter_mut()
        .zip(def_paths(root, source.as_bytes(), &def_ranges))
    {
        d.path = path;
        d.parent = parent;
    }
    // Attribute each reference to its caller (`enclosing_def`: the innermost enclosing function,
    // else the innermost enclosing definition; spec 116), named by that definition's path (spec
    // 119 criterion 8: `T::new`, never the leaf `new`). The
    // reference order is unchanged - `enclosing` is a derived per-reference attribute, never a new
    // sort key, so identical source still yields byte-identical downstream events.
    for (r, &pos) in refs.iter_mut().zip(ref_positions.iter()) {
        r.enclosing = enclosing_def(&def_ranges, pos).map(|i| defs[i].path.clone());
    }
    // Spec 86 criterion 1: mark every definition and reference that falls inside a TEST REGION -
    // every test-attributed range (`test_regions`: a definition directly annotated
    // `#[test]`/`#[cfg(test)]`, an impl block or one the name filter drops included, an extern
    // block, or the whole file opening `#![cfg(test)]`), a definition nested inside one included.
    // Byte-range based, so it is exact regardless of which line a construct starts
    // or ends on; computed AFTER every def's range is known, so a def's own containment check can
    // see siblings and ancestors alike whatever order the tags happened to arrive in.
    // `test_regions` walks the parsed tree's `attribute_item` nodes structurally instead of
    // re-deriving attribute/comment boundaries from characters.
    let regions = test_regions(source.as_bytes(), root, &def_ranges, &dropped);
    if !regions.is_empty() {
        for (d, (range, _, _)) in defs.iter_mut().zip(def_ranges.iter()) {
            d.is_test = regions
                .iter()
                .any(|r| r.start <= range.start && range.end <= r.end);
        }
        for (r, &pos) in refs.iter_mut().zip(ref_positions.iter()) {
            r.is_test = regions.iter().any(|region| region.contains(&pos));
        }
    }
    // Round 6 (`op-u86c1-r5-close-every-remaining-test-shape` item 2): mark every `Module`-kind
    // definition that is an OUT-OF-LINE declaration (`mod name;`, no body of its own - see
    // `Def::is_out_of_line_module`'s doc for why this file's own extraction can never resolve what
    // it names, only flag that it IS one). Independent of the test-region pass above: whether the
    // declaration is ALSO test-attributed is already carried on `is_test` by that same pass (a
    // `mod_item`, in or out of line, is itself a `def_ranges` entry, so `test_regions` already
    // covers it); this only adds the "has no body" fact the events/index layer needs to act on it.
    // Round 7 (`op-u86c1-r7-out-of-line-module-resolution-follows-rust`): an out-of-line
    // declaration's own `#[path = ".."]` attribute, if any, is captured here too
    // ([`out_of_line_path_override`]) - the SAME reason as `is_out_of_line_module` itself: only
    // this file's own parsed tree carries the attribute, so the events/index layer that resolves
    // it needs it carried on the definition, not re-derived from source it no longer has.
    for (d, (range, _, _)) in defs.iter_mut().zip(def_ranges.iter()) {
        if d.kind == Kind::Module {
            if let Some(n) = root.descendant_for_byte_range(range.start, range.end) {
                d.is_out_of_line_module =
                    n.kind() == "mod_item" && n.child_by_field_name("body").is_none();
                if d.is_out_of_line_module {
                    d.path_override = out_of_line_path_override(n, source.as_bytes());
                    // Round 9 (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-
                    // never-excludes`): which enclosing INLINE modules, if any, this declaration
                    // sits nested inside - the events/index layer's `#[path]`-override branch
                    // needs this to compute the correct base directory (`Def::
                    // enclosing_inline_module_path`'s doc).
                    d.enclosing_inline_module_path =
                        enclosing_inline_module_path(n, source.as_bytes());
                }
            }
        }
    }
    // Spec 92 criterion 2 round 4 (review REJECT `adj-u2c2-r3-verdict-reject`, finding
    // `adv-u2c2-partial-marker-unimplemented`): whether this parse hit an ERROR node anywhere -
    // `has_error()` on `tree`'s root is true iff the tree contains a syntax-error or missing-token
    // node, the tree-sitter-native signal that the grammar could not fully parse `source`. Read off
    // the SAME `tree` `test_regions`/the out-of-line-module walk above already parsed (never a
    // second parse), so a malformed file's `defs`/`refs` are carried exactly as far as the parse
    // reached, tagged as partial rather than silently presented as complete.
    let partial = root.has_error();
    Ok(FileSymbols {
        lang,
        defs,
        refs,
        partial,
    })
}

/// THE TEST REGIONS (spec 116): the byte ranges of every TEST-ATTRIBUTED range in the file
/// `source` parsed to `root`, one whose own `#[test]`/`#[cfg(..test..)]` attribute stack sits
/// directly above it ([`preceded_by_test_attribute`]): each definition of `def_ranges` and each
/// one THE NAME FILTER dropped (`dropped`, a `#[cfg(test)] const _: () = { .. }`'s among them);
/// each `foreign_mod_item`, an extern block being a container that yields no definition; and
/// the whole file, `0..source.len()`, when the root opens with an inner `#![cfg(test)]`
/// ([`opens_with_inner_test_attribute`]). A definition NESTED inside one of these ranges (a
/// plain helper with no attribute of its own, living inside a `#[cfg(test)] mod tests { .. }`) is
/// test code too, but it earns that status by CONTAINMENT, not by appearing in this list - the
/// caller checks containment against the ranges this returns, so a doubly-nested item is covered
/// by the SAME outer range without this function needing to recurse.
fn test_regions(
    source: &[u8],
    root: tree_sitter::Node,
    def_ranges: &[(std::ops::Range<usize>, String, Kind)],
    dropped: &[std::ops::Range<usize>],
) -> Vec<std::ops::Range<usize>> {
    let mut regions: Vec<_> = def_ranges
        .iter()
        .map(|(range, _, _)| range)
        .chain(dropped)
        .filter(|range| preceded_by_test_attribute(source, root, range))
        .cloned()
        .collect();
    regions.extend(
        nodes_of_kind(root, "foreign_mod_item")
            .into_iter()
            .filter(|block| node_preceded_by_test_attribute(*block, source))
            .map(|block| block.byte_range()),
    );
    if opens_with_inner_test_attribute(root.named_child(0), source) {
        regions.push(0..source.len());
    }
    regions
}

/// Whether the definition spanning `range` is directly preceded - skipping only comment nodes
/// (`line_comment`, which covers plain `//` and doc `///`/`//!` alike, and `block_comment`) - by
/// an attribute node that GATES THE ITEM'S OWN COMPILATION on `test`: bare `#[test]`, or
/// `#[cfg(...)]` whose predicate names `test` ([`attribute_item_names_test`] /
/// [`cfg_predicate_token_tree_names_test`]). `range` is looked up in the ALREADY-PARSED tree
/// rooted at `root`, and the walk starts from the OUTERMOST node below the root whose range
/// equals it: `descendant_for_byte_range` answers the deepest such node, which for a unit
/// variant is its name child, a node with no sibling, so the lookup climbs every parent of the
/// same range and stops below the root, whose range a file of one item shares. A tagged
/// definition's range is its grammar node's; a tuple field's is its type node's, the grammar
/// giving it no node of its own. The scan walks that node's `prev_sibling()` chain: an
/// `attribute_item` (or comment) is a sibling of the item it decorates in every grammar shape
/// (a top-level item, a `mod` body, an `impl` body, a field or variant list), so this
/// generalizes to any nesting depth without a separate case for each. The walk stops at the first sibling that is neither a comment nor an `attribute_item` -
/// the boundary of the contiguous attribute/comment stack directly above the item - and every
/// attribute in that stack is inspected (not just the nearest), so `#[test]` two attributes above
/// a `#[should_panic]` still matches, and a comment between two stacked attributes never severs
/// the scan.
///
/// Round 4 (review REJECT `adj-u86c1-verdict-reject` round 3, finding
/// `sdet-u86c1-r3-embedded-slash-attribute-plus-trailing-comment-severs-scan`): this REPLACES a
/// hand-rolled LINE-based text scan that had already gone through 5 shape-specific patches (bare
/// `not(test)`/`cfg_attr` in round 1, compound `not()`/`any()` in round 2, a same-line
/// trailing-comment strip in round 2, a `#[...]`-shape-preserving guard and a `find("//")`
/// trailing-comment fallback in round 3) and STILL broke a 6th time: an ordinary
/// `#[doc = "https://..."]` attribute carrying a genuine trailing comment truncated at the URL's
/// OWN `//` instead of the comment's, failed the shape check, and severed the scan before it ever
/// reached a `#[test]` one line further up. Reading the grammar's own parsed nodes instead of
/// re-deriving attribute/string/comment boundaries from characters eliminates the whole defect
/// class by construction - a string literal's content (an embedded `//`, an embedded `]`, a
/// multi-line value) can never perturb where one node ends and the next begins, because the
/// parser already resolved that - rather than requiring a 7th, 8th, ... point patch per new shape.
///
/// Round 5 (review REJECT `adj-u86c1-verdict-reject` round 4, finding
/// `adv-u86c1-r4-inner-cfg-test-attribute-not-recognized`): an OUTER `#[cfg(test)]` (a sibling
/// BEFORE the item it gates) and an INNER `#![cfg(test)]` (the FIRST node INSIDE the item's own
/// body, gating the item that body belongs to - `mod tests { #![cfg(test)] .. }`) are two
/// grammar-distinct shapes for the identical Rust idiom. Two checks now cover both, both riding
/// the SAME shared walk ([`scan_attribute_stack`]) in opposite directions over different
/// sibling sets: `node.prev_sibling()` for an item that sits AFTER an inner attribute as a normal
/// sibling inside a shared body (e.g. a plain helper following `#![cfg(test)]` in the same `mod`'s
/// `declaration_list` - also how the pre-existing OUTER `#[cfg(test)]` shape is found, unchanged);
/// [`leading_inner_test_attribute`]'s `body.named_child(0)` / `next_named_sibling()` for the item
/// the inner attribute itself governs (e.g. the `mod` whose body the `#![cfg(test)]` opens), which
/// has no PRECEDING sibling of its own to walk - the attribute lives inside its body, never before
/// it - so the sibling walk alone can never self-attribute it. `tree-sitter-rust`'s complete set of
/// attribute-bearing node kinds is exactly `attribute_item` and `inner_attribute_item` (grepped
/// `node-types.json`; `attribute` itself is never a sibling - it is always the sole named child of
/// one of the other two), so these two checks are exhaustive.
///
/// The actual test - both directions - is [`node_preceded_by_test_attribute`].
fn preceded_by_test_attribute(
    source: &[u8],
    root: tree_sitter::Node,
    range: &std::ops::Range<usize>,
) -> bool {
    outermost_node(root, range).is_some_and(|node| node_preceded_by_test_attribute(node, source))
}

/// The OUTERMOST node below `root` spanning what `range` spans: `descendant_for_byte_range`
/// answers the deepest node holding `range`, and every parent of that same range is climbed,
/// stopping below the root, whose range a file of one item shares.
fn outermost_node<'t>(
    root: tree_sitter::Node<'t>,
    range: &std::ops::Range<usize>,
) -> Option<tree_sitter::Node<'t>> {
    let mut node = root.descendant_for_byte_range(range.start, range.end)?;
    while let Some(parent) = node
        .parent()
        .filter(|p| p.parent().is_some() && p.byte_range() == node.byte_range())
    {
        node = parent;
    }
    Some(node)
}

/// The core [`preceded_by_test_attribute`] applies once a candidate node is in hand, and
/// [`test_regions`] applies to each extern block's own node: self-attributed test either through the node's OWN leading
/// inner attribute ([`leading_inner_test_attribute`]) or through an outer attribute stack sitting
/// as its sibling immediately before it. A tuple field's `pub` is a sibling too, between its
/// attributes and its type, so the walk first steps over a `visibility_modifier`.
fn node_preceded_by_test_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    let above = match node.prev_sibling() {
        Some(visibility) if visibility.kind() == "visibility_modifier" => visibility.prev_sibling(),
        above => above,
    };
    leading_inner_test_attribute(node, source)
        || attribute_stack_names_test(source, above, |n| n.prev_sibling())
}

/// One impl block read off the parsed tree: its header name (`impl Trait for Type`, or
/// `impl Type` for an inherent impl, each written as in the source with its whitespace collapsed),
/// its whole byte range, its 1-based line, and the `(name, line, byte)` of each item it links to -
/// the trait (when it has one) and the self type, by their base identifier, so a path-qualified
/// (`crate::store::EventStore`) or generic (`Wrapper<T>`) name still links to its definition.
struct ImplBlock {
    name: String,
    range: std::ops::Range<usize>,
    line: u32,
    targets: Vec<(String, u32, usize)>,
}

/// Every `impl_item` anywhere under `root` (an impl nested in a function or module included), in
/// source order, as an [`ImplBlock`]. Keyed on the Rust grammar's own node kind, so a file of any
/// other language has none.
fn impl_blocks(root: tree_sitter::Node, source: &[u8]) -> Vec<ImplBlock> {
    nodes_of_kind(root, "impl_item")
        .into_iter()
        .filter_map(|node| impl_block(node, source))
        .collect()
}

/// Rust's TUPLE FIELDS (spec 116): every field of every tuple struct and tuple variant under
/// `root`, in source order, as its byte range, its leaf and its 1-based line. The grammar gives
/// a tuple field no node of its own (an `ordered_field_declaration_list` holds each field's
/// attributes, visibility and type as siblings), so a field is read off its type node, whose
/// range and line it takes, and its leaf is its index among the list's types, from `0`.
fn tuple_fields(root: tree_sitter::Node) -> Vec<ReadOff> {
    let mut out = Vec::new();
    for list in nodes_of_kind(root, "ordered_field_declaration_list") {
        let mut cursor = list.walk();
        for (index, ty) in list.children_by_field_name("type", &mut cursor).enumerate() {
            out.push((
                ty.byte_range(),
                index.to_string(),
                ty.start_position().row as u32 + 1,
            ));
        }
    }
    out
}

/// Go's EMBEDDED FIELDS (spec 116): every `field_declaration` under `root` that holds no
/// `name`, in source order, as its byte range, its leaf and its 1-based line. Its leaf is the
/// name Go gives it, its type's base identifier ([`base_type_identifier`]); the upstream query
/// tags that identifier a type reference, which the field's range then encloses. A pointer
/// embed (`*T`) holds its `T` as the `type` child itself.
fn embedded_fields(root: tree_sitter::Node, source: &[u8]) -> Vec<ReadOff> {
    nodes_of_kind(root, "field_declaration")
        .into_iter()
        .filter(|field| field.child_by_field_name("name").is_none())
        .filter_map(|field| {
            let leaf = base_type_identifier(field.child_by_field_name("type")?)?;
            read_off(field, leaf, source)
        })
        .collect()
}

/// The definitions a grammar names by a field its query cannot tag (spec 116): every node of
/// the grammar kind `kind` under `root` holding the field `field`, in source order, as its byte
/// range, its leaf, that field's text, and its 1-based line. Go's TYPE ALIASES are every
/// `type_alias` (`type A = B`) by its `name`, which the upstream query tags a type reference
/// the alias's range then encloses; C#'s OPERATORS are every `operator_declaration` by its
/// `operator`, the token (`+`) the grammar gives it in place of a name node.
fn named_by_field(root: tree_sitter::Node, source: &[u8], kind: &str, field: &str) -> Vec<ReadOff> {
    nodes_of_kind(root, kind)
        .into_iter()
        .filter_map(|node| read_off(node, node.child_by_field_name(field)?, source))
        .collect()
}

/// C#'s CONVERSION OPERATORS (spec 116): every `conversion_operator_declaration` under `root`,
/// in source order, as its byte range, its leaf and its 1-based line. Its leaf is its header
/// from `implicit` or `explicit` through its target type, as written with its whitespace
/// collapsed (`implicit operator int`).
fn conversion_operators(root: tree_sitter::Node, source: &[u8]) -> Vec<ReadOff> {
    nodes_of_kind(root, "conversion_operator_declaration")
        .into_iter()
        .filter_map(|op| {
            let mut cursor = op.walk();
            let from = op
                .children(&mut cursor)
                .find(|c| matches!(c.kind(), "implicit" | "explicit"))?;
            let to = op.child_by_field_name("type")?;
            let header = std::str::from_utf8(source.get(from.start_byte()..to.end_byte())?).ok()?;
            Some((
                op.byte_range(),
                normalize_ws(header),
                op.start_position().row as u32 + 1,
            ))
        })
        .collect()
}

/// C#'s INDEXERS (spec 116): every `indexer_declaration` under `root`, in source order, as its
/// byte range, its leaf and its 1-based line. The grammar gives an indexer no name node; its
/// leaf is its `this` token.
fn indexers(root: tree_sitter::Node, source: &[u8]) -> Vec<ReadOff> {
    nodes_of_kind(root, "indexer_declaration")
        .into_iter()
        .filter_map(|indexer| {
            let mut cursor = indexer.walk();
            let this = indexer.children(&mut cursor).find(|c| c.kind() == "this")?;
            read_off(indexer, this, source)
        })
        .collect()
}

/// JS's CLASS FIELDS (spec 116): every `field_definition` under `root`, in source order, as its
/// byte range, its leaf and its 1-based line. The grammar names a field by its `property`, read
/// as a member's name is ([`member_read_off`]); TS's class field, a `public_field_definition`
/// named by its `name`, is tagged by the TS query instead. A function-valued field lowers to a
/// method as a tagged one does ([`is_function_valued`]).
fn class_fields(root: tree_sitter::Node, source: &[u8]) -> Vec<ReadOff> {
    nodes_of_kind(root, "field_definition")
        .into_iter()
        .filter_map(|field| member_read_off(field, field.child_by_field_name("property")?, source))
        .collect()
}

/// TS's VALUELESS ENUM MEMBERS (spec 116): every name an `enum_body` under `root` holds as its
/// own `name` (`enum E { A, "b-c" }`), in source order, as its byte range, its leaf and its
/// 1-based line, each read as a member's name is ([`member_read_off`]) and spanning its name
/// alone. A member with a value is an `enum_assignment`, which the TS query tags.
fn enum_members(root: tree_sitter::Node, source: &[u8]) -> Vec<ReadOff> {
    let mut out = Vec::new();
    for body in nodes_of_kind(root, "enum_body") {
        let mut cursor = body.walk();
        for name in body.children_by_field_name("name", &mut cursor) {
            out.extend(member_read_off(name, name, source));
        }
    }
    out
}

/// The [`ReadOff`] of the JS or TS member `node` named by `name`, its leaf read as a tagged
/// member's is ([`def_leaf`]): a string name's text between its quotes, a private name with
/// its `#`, a numeric one as written; `None` for a computed name (`[k]`), which names nothing
/// statically.
fn member_read_off(
    node: tree_sitter::Node,
    name: tree_sitter::Node,
    source: &[u8],
) -> Option<ReadOff> {
    if name.kind() == "computed_property_name" {
        return None;
    }
    let (range, text, line) = read_off(node, name, source)?;
    Some((range, def_leaf(Some(name), &text), line))
}

/// What a destructuring pattern node binds in THE BINDING WALK (spec 116): the node itself, a
/// bare identifier; each of its named children, a nested or a rest pattern; or the one child in
/// a field, a rename's `value` or a default's `left`. A node kind a grammar's table does not list
/// binds nothing.
enum Binds {
    Itself,
    Children,
    Field(&'static str),
}

/// The JS and TS pattern node kinds and what each binds: `{a}`'s shorthand and a bare
/// identifier bind themselves; an object, an array and a rest (`...r`) pattern are walked down;
/// a rename (`{a: b}`) binds its value and a default (`{a = 1}`, `[x = 1]`) its left. An array
/// hole has no node, and a member or subscript target is unlisted, so each binds nothing.
const JS_PATTERN_BINDINGS: [(&str, Binds); 8] = [
    ("identifier", Binds::Itself),
    ("shorthand_property_identifier_pattern", Binds::Itself),
    ("object_pattern", Binds::Children),
    ("array_pattern", Binds::Children),
    ("rest_pattern", Binds::Children),
    ("pair_pattern", Binds::Field("value")),
    ("assignment_pattern", Binds::Field("left")),
    ("object_assignment_pattern", Binds::Field("left")),
];

/// THE BINDING WALK (spec 116): every bare identifier `pattern` binds under `table`, a
/// grammar's pattern node kinds, pushed onto `out` in source order as its own range, its text
/// and its 1-based line.
fn bound_names(
    pattern: tree_sitter::Node,
    table: &[(&str, Binds)],
    source: &[u8],
    out: &mut Vec<ReadOff>,
) {
    match table.iter().find(|(kind, _)| *kind == pattern.kind()) {
        Some((_, Binds::Itself)) => out.extend(read_off(pattern, pattern, source)),
        Some((_, Binds::Children)) => {
            let mut cursor = pattern.walk();
            let children: Vec<_> = pattern.named_children(&mut cursor).collect();
            for child in children {
                bound_names(child, table, source, out);
            }
        }
        Some((_, Binds::Field(field))) => {
            if let Some(child) = pattern.child_by_field_name(field) {
                bound_names(child, table, source, out);
            }
        }
        None => {}
    }
}

/// JS's and TS's DESTRUCTURED BINDINGS (spec 116), the binding walk's seeds: the name of every
/// `variable_declarator` named by a pattern rather than an identifier (`const {a, b: c} = o`,
/// `const [x] = f()`), and the `left` of every `for_in_statement` that declares (`for (const k of
/// m)`, a `const`, `let` or `var` `kind`), each walked by [`bound_names`] over
/// [`JS_PATTERN_BINDINGS`]. A loop with no `kind` assigns existing names and binds nothing.
fn js_bindings(root: tree_sitter::Node, source: &[u8]) -> Vec<ReadOff> {
    let declared = nodes_of_kind(root, "variable_declarator")
        .into_iter()
        .filter_map(|declarator| declarator.child_by_field_name("name"))
        .filter(|name| name.kind() != "identifier");
    let looped = nodes_of_kind(root, "for_in_statement")
        .into_iter()
        .filter(|for_in| for_in.child_by_field_name("kind").is_some())
        .filter_map(|for_in| for_in.child_by_field_name("left"));
    let mut out = Vec::new();
    for seed in declared.chain(looped) {
        bound_names(seed, &JS_PATTERN_BINDINGS, source, &mut out);
    }
    out
}

/// The Python pattern node kinds and what each binds: a bare identifier binds itself; a tuple
/// (`a, b`), a parenthesized tuple (`(a, b)`), a list (`[c, d]`) and a starred (`*d`) pattern
/// are walked down. An attribute and a subscript target (`self.x`, `d[k]`) are unlisted, so
/// each binds nothing.
const PY_PATTERN_BINDINGS: [(&str, Binds); 5] = [
    ("identifier", Binds::Itself),
    ("pattern_list", Binds::Children),
    ("tuple_pattern", Binds::Children),
    ("list_pattern", Binds::Children),
    ("list_splat_pattern", Binds::Children),
];

/// Python's ASSIGNED NAMES (spec 116) in `scope`, the module or a class body, read off every
/// assignment that is a direct statement of it. An assignment of ONE target ([`single_target`])
/// names one definition spanning the whole assignment, its value included, so a call in that
/// value takes it as caller: with `tagged` true (the module) the Python query tags it a constant
/// and it is skipped here, and with `tagged` false (a class body) it is read off here with the
/// same range. Every other assignment seeds the binding walk: its targets, a chained
/// assignment's (`x = y = f()`) each one, followed down its `right` while that is an
/// assignment, so the far-right value never binds, are each walked by [`bound_names`] over
/// [`PY_PATTERN_BINDINGS`], each bound name spanning its own identifier alone.
fn assigned_names(scope: tree_sitter::Node, source: &[u8], tagged: bool) -> Vec<ReadOff> {
    let mut out = Vec::new();
    let mut cursor = scope.walk();
    let statements: Vec<_> = scope
        .named_children(&mut cursor)
        .filter(|statement| statement.kind() == "expression_statement")
        .collect();
    for statement in statements {
        let mut cursor = statement.walk();
        let assignments: Vec<_> = statement
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "assignment")
            .collect();
        for assignment in assignments {
            if let Some(target) = single_target(assignment) {
                if !tagged {
                    out.extend(read_off(assignment, target, source));
                }
                continue;
            }
            let mut link = Some(assignment);
            while let Some(current) = link {
                if let Some(target) = current.child_by_field_name("left") {
                    bound_names(target, &PY_PATTERN_BINDINGS, source, &mut out);
                }
                link = current
                    .child_by_field_name("right")
                    .filter(|right| right.kind() == "assignment");
            }
        }
    }
    out
}

/// The one target of the Python `assignment` when it has one: its `left` when that is an
/// identifier and its value is absent (`w: int`) or no assignment (`z = 1`), the shape the
/// Python query tags a constant at module level; `None` for a pattern or a chained target.
fn single_target(assignment: tree_sitter::Node) -> Option<tree_sitter::Node> {
    assignment
        .child_by_field_name("left")
        .filter(|left| left.kind() == "identifier")
        .filter(|_| {
            assignment
                .child_by_field_name("right")
                .is_none_or(|right| right.kind() != "assignment")
        })
}

/// Python's CLASS-BODY NAMES (spec 116): the names every assignment directly in a class body
/// under `root` makes ([`assigned_names`]), a nested class's and a decorated one's included,
/// in source order: an assignment of one target its field over the whole assignment, a pattern's
/// or a chain's each bound name over its own identifier. The Python query tags none of them.
fn class_body_names(root: tree_sitter::Node, source: &[u8]) -> Vec<ReadOff> {
    nodes_of_kind(root, "class_definition")
        .into_iter()
        .filter_map(|class| class.child_by_field_name("body"))
        .flat_map(|body| assigned_names(body, source, false))
        .collect()
}

/// Python's TYPE ALIASES (spec 116): every `type_alias_statement` under `root`, in source
/// order, as its byte range, its leaf and its 1-based line. Its leaf is the identifier its
/// `left` type holds, directly (`type Y = int`) or as a generic type's (`type X[T] = int` gives
/// `X`).
fn type_aliases(root: tree_sitter::Node, source: &[u8]) -> Vec<ReadOff> {
    nodes_of_kind(root, "type_alias_statement")
        .into_iter()
        .filter_map(|alias| {
            let named = alias.child_by_field_name("left")?.named_child(0)?;
            let leaf = match named.kind() {
                "generic_type" => named.named_child(0)?,
                _ => named,
            };
            read_off(alias, leaf, source)
        })
        .collect()
}

/// A definition read off the parsed tree: its byte range, its leaf and its 1-based line.
type ReadOff = (std::ops::Range<usize>, String, u32);

/// The [`ReadOff`] of the definition `node` named by the text of `leaf`; `None` when that text
/// is no UTF-8.
fn read_off(node: tree_sitter::Node, leaf: tree_sitter::Node, source: &[u8]) -> Option<ReadOff> {
    Some((
        node.byte_range(),
        leaf.utf8_text(source).ok()?.to_string(),
        node.start_position().row as u32 + 1,
    ))
}

/// Every node of the grammar kind `kind` anywhere under `root`, `root` included, in source
/// order.
fn nodes_of_kind<'t>(root: tree_sitter::Node<'t>, kind: &str) -> Vec<tree_sitter::Node<'t>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == kind {
            out.push(node);
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    out
}

/// The [`ImplBlock`] for one `impl_item` node, or `None` when its self type cannot be read (a
/// syntax-error recovery shape).
fn impl_block(node: tree_sitter::Node, source: &[u8]) -> Option<ImplBlock> {
    let (self_type, trait_name) = impl_halves(node, source)?;
    let name = match trait_name {
        Some(t) => format!("impl {t} for {self_type}"),
        None => format!("impl {self_type}"),
    };
    let targets = node
        .child_by_field_name("trait")
        .into_iter()
        .chain(node.child_by_field_name("type"))
        .filter_map(base_type_identifier)
        .filter_map(|id| {
            Some((
                collapsed(id, source)?,
                id.start_position().row as u32 + 1,
                id.start_byte(),
            ))
        })
        .collect();
    Some(ImplBlock {
        name,
        range: node.byte_range(),
        line: node.start_position().row as u32 + 1,
        targets,
    })
}

/// The two halves of the `impl_item` `node`'s header, each as written with its whitespace
/// collapsed ([`collapsed`]): its self type and, for a trait impl, its trait; `None` when its
/// self type, or the trait it names, cannot be read (a syntax-error recovery shape).
fn impl_halves(node: tree_sitter::Node, source: &[u8]) -> Option<(String, Option<String>)> {
    let self_type = collapsed(node.child_by_field_name("type")?, source)?;
    let trait_name = match node.child_by_field_name("trait") {
        Some(t) => Some(collapsed(t, source)?),
        None => None,
    };
    Some((self_type, trait_name))
}

/// The source text of `node` with each whitespace run one space and its ends trimmed
/// ([`normalize_ws`]); `None` when that text is no UTF-8.
fn collapsed(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    Some(normalize_ws(node.utf8_text(source).ok()?))
}

/// THE PATH PASS (spec 119, IDS FOLLOW THE LANGUAGE'S PATH): each definition's id path in its
/// file, index-aligned with `def_ranges`, the ranges THE NAME FILTER leaves, so a definition it
/// drops (`const _: () = { .. }`) contributes nothing and what it held takes the next container.
/// A definition's container is the innermost definition whose range STRICTLY holds its own
/// ([`container_of`]), a range two definitions share holding nothing (Go's `const a, b = f(), g()`
/// makes siblings, and what both hold is their sibling), and a C# file-scoped namespace, whose
/// node holds only its name, holds every declaration after it. Its path is the container's
/// prefix, `::`, its leaf, `.` before a `Field` or `Property` leaf; at file level, its leaf. A
/// container's prefix is its own path, but that an impl gives its self type, or `<T as Trait>`,
/// in place of its header ([`impl_halves`]), so `impl T { fn new() }` gives `impl T` and
/// `T::new`. Two definitions take a path of their own: a Go method is `<receiver base
/// type>::<name>` ([`go_receiver`]), and a JS or TS named function, generator or class value
/// whose name is the leaf of the declarator, assignment, pair or class field holding it
/// ([`is_named_value`]) takes that container's path (`const f = function f() {}` is `f`).
///
/// Beside each path it answers the definition's PARENT (spec 119, CONTAINS IS STRUCTURE), the
/// path its `CONTAINS` comes from: its container's path, but that a named value takes its
/// container's parent, being one definition with it, and a Go method the path of its receiver
/// base type when a definition of the file sits there; `None` at file level.
fn def_paths(
    root: tree_sitter::Node,
    source: &[u8],
    def_ranges: &[(std::ops::Range<usize>, String, Kind)],
) -> Vec<(String, Option<String>)> {
    let nodes: Vec<_> = def_ranges
        .iter()
        .map(|(range, _, _)| outermost_node(root, range).filter(|n| n.byte_range() == *range))
        .collect();
    let spans: Vec<_> = def_ranges
        .iter()
        .zip(&nodes)
        .map(|((range, _, _), node)| match node {
            Some(n) if n.kind() == "file_scoped_namespace_declaration" => range.start..source.len(),
            _ => range.clone(),
        })
        .collect();
    let containers: Vec<_> = (0..spans.len()).map(|i| container_of(&spans, i)).collect();
    // A container's span strictly holds what it contains, so the longest spans are resolved first.
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(spans[i].len()));
    let receivers: Vec<_> = nodes
        .iter()
        .map(|node| node.and_then(|n| go_receiver(n, source)))
        .collect();
    let mut paths = vec![String::new(); def_ranges.len()];
    let mut prefixes = vec![String::new(); def_ranges.len()];
    let mut parents: Vec<Option<String>> = vec![None; def_ranges.len()];
    for i in order {
        let (_, leaf, kind) = &def_ranges[i];
        let under = |prefix: Option<&str>, sep: &str, leaf: &str| match prefix {
            Some(prefix) => format!("{prefix}{sep}{leaf}"),
            None => leaf.to_string(),
        };
        let prefix = containers[i].map(|c| prefixes[c].as_str());
        let sep = if matches!(kind, Kind::Field | Kind::Property) {
            "."
        } else {
            "::"
        };
        let path = match (nodes[i], containers[i]) {
            (Some(n), Some(c)) if is_named_value(n, leaf, &def_ranges[c].1) => {
                parents[i] = parents[c].clone();
                paths[c].clone()
            }
            _ => {
                parents[i] = containers[i].map(|c| paths[c].clone());
                match &receivers[i] {
                    Some(receiver) => format!("{receiver}::{leaf}"),
                    None => under(prefix, sep, leaf),
                }
            }
        };
        let prefix_out = match nodes[i]
            .filter(|n| n.kind() == "impl_item")
            .and_then(|n| impl_halves(n, source))
        {
            Some((self_type, Some(t))) => under(prefix, "::", &format!("<{self_type} as {t}>")),
            Some((self_type, None)) => under(prefix, "::", &self_type),
            None => path.clone(),
        };
        paths[i] = path;
        prefixes[i] = prefix_out;
    }
    // A Go method's parent is the definition at its receiver type's path, which only a file-level
    // definition's path can equal, once every path is known; else the file.
    for (parent, receiver) in parents.iter_mut().zip(receivers) {
        if let Some(receiver) = receiver {
            *parent = paths.contains(&receiver).then_some(receiver);
        }
    }
    paths.into_iter().zip(parents).collect()
}

/// The index of the definition whose span STRICTLY holds span `i` of `spans` most narrowly - the
/// innermost container, a span equal to `i`'s never one - among the spans exactly one definition
/// has, a span two equal-range siblings share holding nothing; `None` at file level.
fn container_of(spans: &[std::ops::Range<usize>], i: usize) -> Option<usize> {
    let inner = &spans[i];
    spans
        .iter()
        .enumerate()
        .filter(|(_, span)| {
            let holds = |at: &usize| (span.start..=span.end).contains(at);
            *span != inner && holds(&inner.start) && holds(&inner.end)
        })
        .filter(|(_, span)| spans.iter().filter(|other| other == span).count() == 1)
        .min_by_key(|(_, span)| span.len())
        .map(|(j, _)| j)
}

/// Whether `node`, a definition named `leaf`, is a NAMED VALUE (spec 119): a JS or TS
/// `function_expression`, `generator_function` or `class` (node types no other registered grammar
/// has) that is the value of a declarator, an assignment, a pair or a class field whose
/// definition, its container, is named `leaf` too (`container_leaf`), so the two are one
/// definition at the container's path.
fn is_named_value(node: tree_sitter::Node, leaf: &str, container_leaf: &str) -> bool {
    matches!(
        node.kind(),
        "function_expression" | "generator_function" | "class"
    ) && node.parent().is_some_and(|p| {
        matches!(
            p.kind(),
            "variable_declarator"
                | "assignment_expression"
                | "pair"
                | "field_definition"
                | "public_field_definition"
        )
    }) && leaf == container_leaf
}

/// The receiver base type of `node` when it holds a `receiver`, which only a Go
/// `method_declaration` does among the registered grammars: the type of its receiver's parameter
/// by its base identifier ([`base_type_identifier`]), pointer, package qualifier and type
/// arguments stripped (`(r *pkg.T[K])` gives `T`); `None` for any other node, or a receiver naming
/// no single type.
fn go_receiver(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let receiver = node.child_by_field_name("receiver")?;
    let mut cursor = receiver.walk();
    let parameter = receiver
        .named_children(&mut cursor)
        .find(|c| c.kind() == "parameter_declaration")?;
    let base = base_type_identifier(parameter.child_by_field_name("type")?)?;
    Some(base.utf8_text(source).ok()?.to_string())
}

/// The identifier a type names, through its path (`a::b::T` -> `T`, Go's `pkg.T` -> `T`),
/// generic arguments (`T<U>` -> `T`, Go's `T[U]` -> `T`) and reference or pointer (`&T` -> `T`,
/// Go's `*T` -> `T`, whose grammar holds its pointee as its one named child, under no field);
/// `None` for a shape that names no single type (a tuple, a slice, a function type).
fn base_type_identifier(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cur = node;
    loop {
        cur = match cur.kind() {
            "type_identifier" => return Some(cur),
            "scoped_type_identifier" | "qualified_type" => cur.child_by_field_name("name")?,
            "generic_type" | "reference_type" => cur.child_by_field_name("type")?,
            "pointer_type" => cur
                .child_by_field_name("type")
                .or_else(|| cur.named_child(0))?,
            _ => return None,
        };
    }
}

/// Whether `node` (a definition such as a `mod_item`, `function_item`, `impl_item`, ...) is
/// self-attributed test THROUGH ITS OWN BODY: Rust's `#![..]` inner-attribute form governs the
/// item whose body it opens, not a sibling item - `mod tests { #![cfg(test)] fn helper() {} }`
/// parses `#![cfg(test)]` as the FIRST named child of the `mod`'s own `declaration_list`, never as
/// a sibling of the `mod_item` node itself, so [`preceded_by_test_attribute`]'s sibling walk (which
/// only ever looks at what comes BEFORE `node`) can never see it. Reads `node`'s `body` field
/// (present on every item kind that can carry inner attributes - `mod_item`, `function_item`,
/// `impl_item`, `trait_item`, ...; absent, hence always `false`, on a leafy definition kind such as
/// a `const_item` that has no body to open one in) and walks forward from its first named child
/// ([`opens_with_inner_test_attribute`], the file root's read too) via the SAME
/// [`scan_attribute_stack`] the sibling-before case uses, so a doc comment or a stacked
/// `#![allow(..)]` ahead of the real `#![cfg(test)]` never hides it either.
///
/// It counts INNER attributes only (spec 116): an OUTER `#[cfg(test)]` a body opens with governs
/// the member, variant, method, statement or item after it, never the body's owner, so
/// `struct S { #[cfg(test)] probe: u32, real: u32 }` keeps `S` and `real` production and only
/// `probe`, by its own sibling walk, is test code.
fn leading_inner_test_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = node.child_by_field_name("body") else {
        return false;
    };
    opens_with_inner_test_attribute(body.named_child(0), source)
}

/// Whether the leading attribute stack read forward from `first`, a body's or the file root's
/// first named child, holds an INNER attribute naming `test` (`#![cfg(test)]`); an outer
/// `#[cfg(test)]` there governs the item after it, never the body or the file. The one inner-only
/// count [`leading_inner_test_attribute`] reads at a definition's body and [`test_regions`] at
/// the root.
fn opens_with_inner_test_attribute(first: Option<tree_sitter::Node>, source: &[u8]) -> bool {
    scan_attribute_stack(
        source,
        first,
        |n| n.next_named_sibling(),
        |item, s| {
            (item.kind() == "inner_attribute_item" && attribute_item_names_test(item, s))
                .then_some(())
        },
    )
    .is_some()
}

/// The one walk shared by [`preceded_by_test_attribute`] (backward over `prev_sibling`),
/// [`opens_with_inner_test_attribute`] (forward over `next_named_sibling`, from a body's or the
/// file root's first named child), [`is_proc_macro_fn`] and (round 7) [`out_of_line_path_override`]: starting at `first`, follow `advance`
/// across a contiguous run of comment (`line_comment`, `block_comment`) and attribute
/// (`attribute_item`, `inner_attribute_item`) nodes, stopping at the first node that is neither -
/// the boundary of the stack. `visit` is asked of every attribute node encountered along the way
/// (not merely the nearest one), and the LAST `Some` it returns wins - so a comment or an unrelated
/// attribute sitting between two stacked attributes never severs the scan, and (for
/// [`attribute_stack_names_test`]'s boolean-shaped `visit`) a match two attributes further along is
/// never hidden by a nearer one that does not match.
fn scan_attribute_stack<'a, T>(
    source: &[u8],
    first: Option<tree_sitter::Node<'a>>,
    advance: impl Fn(tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>>,
    mut visit: impl FnMut(tree_sitter::Node<'a>, &[u8]) -> Option<T>,
) -> Option<T> {
    let mut found = None;
    let mut cur = first;
    while let Some(node) = cur {
        match node.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" | "inner_attribute_item" => {
                if let Some(v) = visit(node, source) {
                    found = Some(v);
                }
            }
            _ => break,
        }
        cur = advance(node);
    }
    found
}

/// [`scan_attribute_stack`] applied to the "does any attribute in the stack gate on `test`"
/// question [`preceded_by_test_attribute`]'s sibling walk asks -
/// [`attribute_item_names_test`] is the `visit` closure, and presence-of-a-match (`Some(())`)
/// collapses to the boolean its caller wants.
fn attribute_stack_names_test<'a>(
    source: &[u8],
    first: Option<tree_sitter::Node<'a>>,
    advance: impl Fn(tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>>,
) -> bool {
    scan_attribute_stack(source, first, advance, |n, s| {
        attribute_item_names_test(n, s).then_some(())
    })
    .is_some()
}

/// Round 7 (`op-u86c1-r7-out-of-line-module-resolution-follows-rust`): the string value of a
/// `#[path = ".."]` attribute directly governing `node` - an out-of-line `mod_item`, which (having
/// no body of its own) can never carry an INNER `#![path]` the way a test attribute can, so this
/// only ever walks the OUTER (`prev_sibling`) direction, unlike `node_preceded_by_test_attribute`'s
/// two-directional check. Reuses the SAME [`scan_attribute_stack`] walk `attribute_stack_names_test`
/// runs (never a second, hand-rolled stack scan), so a `#[cfg(test)]` and a `#[path = ".."]`
/// stacked over the same declaration in either order are both found regardless of which is nearer.
fn out_of_line_path_override(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    scan_attribute_stack(
        source,
        node.prev_sibling(),
        |n| n.prev_sibling(),
        path_attribute_value,
    )
}

/// Round 9 (`op-u86-c1-path-attribute-contract-is-rustc-s-and-unresolvable-never-excludes`): the
/// `/`-joined chain of every ancestor `mod_item` of `node` that is INLINE (carries a `body`
/// field, `mod outer { .. }`, as opposed to the out-of-line `mod outer;`), outermost first.
/// Walks `node.parent()` to the file's root, skipping every non-`mod_item` ancestor (a
/// `declaration_list` wrapper, in particular) and every OUT-OF-LINE ancestor `mod_item`
/// encountered along the way (an out-of-line declaration has no `body` of its own in THIS
/// file's tree at all, so it can never be an ancestor here, defensive only). `None` when `node`
/// sits directly at the file's own top level (no inline-module ancestor found), the
/// overwhelmingly common case, and the one every pre-round-9 fixture exercised.
fn enclosing_inline_module_path(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut chain = Vec::new();
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "mod_item" && n.child_by_field_name("body").is_some() {
            if let Some(name) = n
                .child_by_field_name("name")
                .and_then(|nm| nm.utf8_text(source).ok())
            {
                chain.push(name.to_string());
            }
        }
        current = n.parent();
    }
    if chain.is_empty() {
        None
    } else {
        chain.reverse();
        Some(chain.join("/"))
    }
}

/// Whether a parsed `attribute_item` node is a `#[path = "value"]` key-value attribute - the ONLY
/// shape `path` ever takes (unlike `cfg`'s parenthesized `arguments`, `path` uses the grammar's
/// `value` field; see `tree-sitter-rust`'s own `node-types.json`, `attribute`'s `value` field) -
/// and if so, its string literal's content with the surrounding quote characters stripped. `None`
/// for every other attribute name, and for a `path` attribute whose value is not a plain
/// double-quoted string literal (round-7 scope: rustc itself requires one here, so this is
/// defensive only, never expected to fire on real source).
fn path_attribute_value(item: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let (attribute, name) = named_attribute(item, source)?;
    if name != "path" {
        return None;
    }
    let value = attribute.child_by_field_name("value")?;
    let text = value.utf8_text(source).ok()?;
    Some(text.trim_matches('"').to_string())
}

/// Whether a parsed `attribute_item` node (an outer `#[...]` attribute) gates the tagged item's
/// OWN COMPILATION on `test` being set - the question [`preceded_by_test_attribute`] actually
/// needs, not merely whether its text mentions the word `test` anywhere. An `attribute_item`'s
/// sole named child is an `attribute` node, whose own first named child names the attribute
/// (`test`, `cfg`, `cfg_attr`, `doc`, `allow`, ...) and whose OPTIONAL `arguments` field (present
/// only on a parenthesized attribute) is the `token_tree` this walks structurally for `cfg`. Three
/// cases, mirroring real Rust semantics:
/// - `test` (bare `#[test]`, the only shape the real attribute ever takes - it accepts no
///   arguments): always gates on test.
/// - `#[cfg(PRED)]` (name `cfg`): matches iff `PRED` [`cfg_predicate_token_tree_names_test`]s.
/// - `#[cfg_attr(PRED, ..)]` (name `cfg_attr`) and everything else (`#[allow(..)]`,
///   `#[derive(..)]`, `#[doc = ".."]`, ...): NEVER matches, regardless of what `PRED`/the args
///   name. `cfg_attr` conditionally attaches its trailing attribute(s) - it does not gate
///   compilation of the tagged item itself, so an item under `#[cfg_attr(test, derive(Debug))]`
///   is compiled unconditionally and is unambiguously product code.
fn attribute_item_names_test(item: tree_sitter::Node, source: &[u8]) -> bool {
    match named_attribute(item, source) {
        Some((_, "test")) => true,
        Some((attribute, "cfg")) => attribute
            .child_by_field_name("arguments")
            .is_some_and(|args| cfg_predicate_token_tree_names_test(args, source)),
        _ => false,
    }
}

/// The `attribute` node an attribute item (`#[..]` or `#![..]`) holds, its sole named child,
/// with the name that attribute opens with (`test`, `cfg`, `path`, `proc_macro`, ...), its own
/// first named child; `None` for an item holding neither.
fn named_attribute<'t, 's>(
    item: tree_sitter::Node<'t>,
    source: &'s [u8],
) -> Option<(tree_sitter::Node<'t>, &'s str)> {
    let attribute = item.named_child(0)?;
    let name = attribute
        .named_child(0)?
        .utf8_text(source)
        .unwrap_or_default();
    Some((attribute, name))
}

/// Whether a `#[cfg(..)]` attribute's parenthesized argument list, `tt` (a `token_tree` node,
/// e.g. the parse of `"(test)"`, `"(all(test, feature = \"x\"))"`, `"(not(test))"`,
/// `"(all(not(test), feature = \"x\"))"`, `"(any(debug_assertions, test))"`) GATES the tagged
/// item's compilation ON `test` being set. `cfg` always takes exactly ONE predicate argument, so
/// this reads the "names test" half of [`token_tree_facts`] - the "is pure test algebra" half it
/// also computes exists only so a NESTED `not(..)` can invert soundly (see
/// [`predicate_group_facts`]'s doc), and is never needed at this, the outermost, call.
fn cfg_predicate_token_tree_names_test(tt: tree_sitter::Node, source: &[u8]) -> bool {
    token_tree_facts(tt, source).0
}

/// [`predicate_group_facts`] applied to a combinator's own parenthesized `token_tree` ARGUMENT
/// (rather than an already-split GROUP): `cfg`/`not` always take exactly one predicate argument,
/// so this reads the token tree's single top-level [`comma_separated_groups`] group. `(false,
/// false)` when the token tree is empty (never test-only, and trivially pure by having nothing
/// impure in it - though this shape does not arise from any real `cfg` syntax).
fn token_tree_facts(tt: tree_sitter::Node, source: &[u8]) -> (bool, bool) {
    comma_separated_groups(tt)
        .first()
        .map(|group| predicate_group_facts(group, source))
        .unwrap_or((false, false))
}

/// Whether one comma-separated argument GROUP (a `Vec` of sibling nodes with no top-level comma
/// between them - see [`comma_separated_groups`]) inside a `cfg` predicate BOTH (a) names `test` -
/// the predicate is satisfiable only when `test` is set - and (b) is PURE test algebra - built
/// EXCLUSIVELY from `test`/`not`/`all`/`any`, naming no other cfg atom (a feature flag,
/// `target_os`, a bare `debug_assertions`, ...) anywhere in it. Computed TOGETHER in one minimal
/// recursive descent over the cfg predicate grammar `ident | not(P) | all(P, ...) | any(P, ...)`,
/// never as two separate walks over the identical shape (a round-6 review finding /
/// `docs/audit`'s own duplication scan: an earlier version of this fix shipped "names test" and
/// "is pure" as two near-identical functions, exactly the duplicate-implementation shape this
/// repo's own audit exists to catch) - `not(P)`'s "names test" answer can only be obtained by
/// inverting P's own "names test" answer WHEN P is ALSO pure, so the `not` arm needs both facts
/// about P at once:
/// - a bare identifier (or a `key = "value"` attribute, e.g. `feature = "x"` - the leading
///   identifier plus a non-`token_tree` value node, so the `not`/`all`/`any` match below never
///   fires) names test, and is pure, iff that LEADING identifier is EXACTLY `test` - so a
///   similarly-spelled but distinct identifier (`testing`, `test_helper`) or a value merely
///   spelled `"test"` (`feature = "test"`) never matches either fact - only the identifier the
///   parser resolved as the predicate's own name is ever compared, never a substring of an
///   unrelated string literal.
/// - `not(P)` (leading identifier `not` followed by its own `token_tree` argument) is pure iff P
///   is, and names test iff P is pure AND P does NOT name test: `not(test)` never names test
///   (`#[cfg(not(test))]` is Rust's standard idiom for the PRODUCTION-only half of a dual-cfg
///   mock construct - the item it guards compiles whenever `test` is NOT set, so it is
///   definitionally the item that SHIPS), and `not(not(test))` (double negation) DOES name test.
///   A MIXED inner predicate - `not(feature = "x")`, naming no `test` anywhere and therefore NOT
///   pure - is never inverted: `feature = "x"`'s own truth is independent of `test` altogether
///   (it can be on or off regardless of `test`), so inverting its unrelated answer would tell you
///   nothing sound about test-exclusivity; the item it guards compiles whenever `feature` is off,
///   in EITHER a test or a non-test build, so it is definitively NOT test-only (a round-6 review
///   finding: reading an earlier, un-guarded `!inner_names_test` as a general "P is not
///   itself test, so not(P) IS test" leap - sound only for a pure `test` subtree - wrongly marked
///   `#[cfg(not(feature = "x"))] fn real_client() {}`, an ordinary product function, as test code
///   and excluded it from the graph).
/// - `all(P1, .., Pn)` (every conjunct must hold to compile) names test iff ANY Pi does - one
///   conjunct requiring `test` is enough to make the WHOLE predicate satisfiable only under test
///   (`all(test, feature = "x")`), and that holds wherever the `not(test)` production-half idiom
///   sits too: `all(not(test), feature = "x")` still means "compiles whenever test is NOT set",
///   never test-only, because its `not(test)` conjunct answers `false`. Pure iff EVERY Pi is
///   (one impure conjunct is enough to make an ENCLOSING `not`'s inversion of this whole group
///   unsound, even though `all` itself never inverts anything - purity must still propagate
///   outward).
/// - `any(P1, .., Pn)` (any ONE disjunct is enough to compile) names test iff EVERY Pi does - a
///   disjunct that does not need `test` (`any(debug_assertions, test)`) gives the item a path to
///   compile with `test` unset (a debug-only helper that ships in every non-release build), so the
///   whole predicate is not test-only even though one disjunct names the `test` token. Pure iff
///   EVERY Pi is, for the same outward-propagation reason as `all`.
fn predicate_group_facts(group: &[tree_sitter::Node], source: &[u8]) -> (bool, bool) {
    let Some(head) = group.first() else {
        return (false, false);
    };
    let name = head.utf8_text(source).unwrap_or_default();
    let combinator_args = group.get(1).filter(|n| n.kind() == "token_tree");
    match (name, combinator_args) {
        ("not", Some(&args)) => {
            let (inner_names_test, inner_pure) = token_tree_facts(args, source);
            (inner_pure && !inner_names_test, inner_pure)
        }
        ("all", Some(&args)) => {
            let facts: Vec<(bool, bool)> = comma_separated_groups(args)
                .iter()
                .map(|g| predicate_group_facts(g, source))
                .collect();
            (
                facts.iter().any(|&(names_test, _)| names_test),
                facts.iter().all(|&(_, pure)| pure),
            )
        }
        ("any", Some(&args)) => {
            let facts: Vec<(bool, bool)> = comma_separated_groups(args)
                .iter()
                .map(|g| predicate_group_facts(g, source))
                .collect();
            (
                facts.iter().all(|&(names_test, _)| names_test),
                facts.iter().all(|&(_, pure)| pure),
            )
        }
        _ => {
            let is_test = name == "test";
            (is_test, is_test)
        }
    }
}

/// The DIRECT named children of a `cfg`/`not`/`all`/`any` combinator's own parenthesized
/// argument-list node, `tt` (a `token_tree`), split into its top-level comma-separated GROUPS: a
/// nested sub-predicate's own parens are already grouped into a single nested `token_tree` child
/// by the grammar itself (`not(test)` parses as the two named children `identifier("not")` and
/// `token_tree("(test)")`, never a flat run of tokens), so this needs no depth tracking of its
/// own - only a `,` directly among `tt`'s children marks a group boundary, and `,` is always
/// anonymous so it is never itself collected into a group. Every other anonymous token (`(`, `)`,
/// the `=` in a `key = "value"` argument, ...) is dropped - the named nodes on either side of it
/// are what a caller inspects. Each returned group is a `Vec` (never a single node) because a
/// `key = "value"` argument is TWO sibling named nodes (`identifier`, `string_literal`) with no
/// grouping node of their own, so `predicate_group_facts` must see both to read the leading
/// identifier.
fn comma_separated_groups(tt: tree_sitter::Node) -> Vec<Vec<tree_sitter::Node>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    let mut cursor = tt.walk();
    for child in tt.children(&mut cursor) {
        if child.kind() == "," {
            groups.push(std::mem::take(&mut current));
        } else if child.is_named() {
            current.push(child);
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// The index in `def_ranges` of the reference at `pos`'s CALLER, whose `defs` entry at that
/// index names it by its path (spec 119 criterion 8): the innermost FUNCTION-LIKE definition
/// (`Function`, `Method`, `Constructor`, `Macro`) whose byte range contains `pos`, else the
/// innermost definition of any other kind that does, else `None` when `pos` lies outside every
/// definition (a top-level reference such as an import or a module-level call), the file then
/// being the caller. A `Declaration` is never a caller: its range is skipped, so a reference in
/// a signature takes the next enclosing definition. Each definition tag carries the byte range of
/// the WHOLE tagged construct - the function body included, not just its name span - so a
/// reference inside a body falls within its definition's range. "Innermost" is the smallest
/// containing range, so a reference in a nested function attributes to the nested one, not its
/// outer scope, while a data definition, impl, trait or module nested in a function leaves its
/// references the function's. Deterministic for identical input: ties on span break on the range
/// start, then the definition name.
fn enclosing_def(
    def_ranges: &[(std::ops::Range<usize>, String, Kind)],
    pos: usize,
) -> Option<usize> {
    def_ranges
        .iter()
        .enumerate()
        .filter(|(_, (range, _, kind))| *kind != Kind::Declaration && range.contains(&pos))
        .min_by_key(|(_, (range, name, kind))| {
            let function_like = matches!(
                kind,
                Kind::Function | Kind::Method | Kind::Constructor | Kind::Macro
            );
            (!function_like, range.end - range.start, range.start, name)
        })
        .map(|(i, _)| i)
}

/// Where [`locate_extent`] bounds a definition's body. Each variant counts only the definitions
/// at the recorded path, when one is given ([`Recorded::path`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Extent {
    /// The 1-based, inclusive line range `[start, end]` of the definition's whole construct, its
    /// body included: `start` the recorded line when a definition still sits there, else the live
    /// line a drifted one healed to.
    Bounded { start: u32, end: u32 },
    /// No production definition of the name and kind sits at the recorded line or anywhere else
    /// in the file, nor a lone one of the name at that line (deleted, not merely moved).
    Missing,
    /// The recorded line drifted and production definitions of the name and kind sit at this many
    /// distinct lines, so which one moved cannot be told.
    Ambiguous(usize),
}

/// A definition as its graph node records it, the one [`locate_extent`] bounds.
#[derive(Clone, Copy, Debug)]
pub struct Recorded<'a> {
    /// Its leaf, the node's `name`.
    pub name: &'a str,
    /// Its id path in its file, the node id's text after the file (spec 119, IDS FOLLOW THE
    /// LANGUAGE'S PATH); `None` for an id holding no `::`, found by name, kind and line alone.
    pub path: Option<&'a str>,
    /// Its kind, a `kind_str` rendering, as the node carries it.
    pub kind: &'a str,
    /// Its recorded 1-based line.
    pub line: u32,
}

/// THE ONE EXTENT a `rigger graph --show` body is bounded by (spec 119, ONE EXTENT): [`extract`]
/// runs over `source` under the INJECTED grammar and tags query, exactly as the code graph is
/// extracted, and the `recorded` definition - named `name`, of kind `kind`, recorded at `line`,
/// and, when its `path` is given, at that id path ([`Def::path`]), so a drifted `T::new` never
/// heals onto `U::new` - is found among the production definitions - those not `is_test`, as
/// `extract_events` drops them, since the graph holds no other - by three tiers, in order, its
/// [`Def::end_line`] bounding the body, every tier reading only the definitions of the name at
/// the given path:
///
/// 1. name, kind and line: the recorded site still holds it; when several do, the widest
///    `end_line` wins.
/// 2. name and line, the kind ignored, when exactly one definition matches: a definition whose
///    kind changed in place since the last ingest still bounds at its recorded line.
/// 3. name and kind at any line (spec 92): the recorded line drifted, and healing is safe only
///    when one distinct line holds them (the widest `end_line` there winning); more lines are
///    [`Extent::Ambiguous`], none [`Extent::Missing`].
///
/// The kind is compared through `kind_str`, so a field beside a same-named fn, or a variant
/// inside its same-named enum, never widens or blocks a tier-1 or tier-3 match; at the recorded
/// line tier 2 ignores the kind, so a lone same-named field there bounds the body when it sits at
/// the path sought: with no `path` given, any such field; with one, only a field whose own path is
/// that path - a field under a named container (`S.h`) never matches a function's (`h`), while a
/// file-level field, under an anonymous class or a binding the name filter drops (Go's `var _`),
/// has its leaf as its path and does. An `Err` is a grammar or query that cannot run over the
/// source.
pub fn locate_extent(
    source: &str,
    lang: Lang,
    ts_language: &tree_sitter::Language,
    tags_query: &str,
    recorded: Recorded,
) -> Result<Extent, String> {
    let Recorded {
        name,
        path,
        kind,
        line,
    } = recorded;
    let symbols = extract(source, lang, ts_language, tags_query)?;
    let named: Vec<&Def> = symbols
        .defs
        .iter()
        .filter(|d| !d.is_test && d.name == name && path.is_none_or(|path| d.path == path))
        .collect();
    // The widest end of each line holding a definition of the name and kind: tier 1 reads the
    // recorded line's, tier 3 the one line's when one is all there is.
    let mut of_kind: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    for d in named.iter().filter(|d| kind_str(d.kind) == kind) {
        let end = of_kind.entry(d.line).or_insert(d.end_line);
        *end = (*end).max(d.end_line);
    }
    if let Some(&end) = of_kind.get(&line) {
        return Ok(Extent::Bounded { start: line, end });
    }
    if let [only] = named.iter().filter(|d| d.line == line).collect::<Vec<_>>()[..] {
        return Ok(Extent::Bounded {
            start: line,
            end: only.end_line,
        });
    }
    Ok(match of_kind.len() {
        0 => Extent::Missing,
        1 => {
            let (start, end) = of_kind.into_iter().next().expect("one line");
            Extent::Bounded { start, end }
        }
        n => Extent::Ambiguous(n),
    })
}

/// The byte offset at which each 0-based line begins. `offsets[0]` is always `0`; each `\n` opens
/// the next line at the byte after it. [`Source`] holds them, so [`def_of`] maps a definition's
/// last byte to its end line.
fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

/// The 0-based line containing byte offset `byte`, by binary search over the line-start offsets:
/// the count of line starts at or before `byte`, minus one, is that line's index.
fn line_of_byte(line_starts: &[usize], byte: usize) -> u32 {
    line_starts
        .partition_point(|&s| s <= byte)
        .saturating_sub(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::{Kind, Lang};

    #[test]
    fn extracts_a_rust_definition_and_a_reference() {
        let src = "fn parse(x: u8) -> u8 { x }\nfn caller() { parse(1); }\n";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();
        // The definition `parse` is found with its kind and 1-based line.
        assert!(fs
            .defs
            .iter()
            .any(|d| d.name == "parse" && d.kind == Kind::Function && d.line == 1));
        // The call site `parse(1)` on line 2 is a reference, not a definition.
        assert!(fs.refs.iter().any(|r| r.name == "parse" && r.line == 2));
        // The extracted file carries the language it was parsed as.
        assert_eq!(fs.lang, Lang::Rust);
    }

    /// Gap 104: the registered Rust grammar records every impl block as an `Impl` definition
    /// named by its header, attributes the impl's trait and self type to that definition (the
    /// impl -> trait and impl -> type links), and records a path-qualified or turbofish call
    /// (`Type::open()`, `crate::m::f()`, `make::<T>()`) as a call reference like a plain one.
    #[test]
    fn rust_impl_blocks_are_definitions_linked_to_their_trait_and_type_and_path_calls_are_references(
    ) {
        let src = "\
pub trait EventStore { fn append(&self); }
pub struct Sqlite;
impl EventStore for Sqlite { fn append(&self) { helper(); } }
impl crate::store::EventStore for Kurrent { fn append(&self) {} }
impl<T> EventStore for Wrapper<T> { fn append(&self) {} }
impl Sqlite { pub fn open() -> Self { crate::sqlite::open_connection(); Sqlite } }
pub fn open_store() -> Sqlite { let s = Sqlite::open(); s.append(); make::<u8>(); s }
";
        let fs = crate::symbol_index::extracted("rs", src);
        let impls: Vec<(&str, u32)> = fs
            .defs
            .iter()
            .filter(|d| d.kind == Kind::Impl)
            .map(|d| (d.name.as_str(), d.line))
            .collect();
        assert_eq!(
            impls,
            vec![
                ("impl EventStore for Sqlite", 3),
                ("impl crate::store::EventStore for Kurrent", 4),
                ("impl EventStore for Wrapper<T>", 5),
                ("impl Sqlite", 6),
            ],
            "every impl block is a definition named by its header; got {:?}",
            fs.defs
        );
        let refs: Vec<(&str, u32, Option<&str>)> = fs
            .refs
            .iter()
            .map(|r| (r.name.as_str(), r.line, r.enclosing.as_deref()))
            .collect();
        for expected in [
            ("EventStore", 3, Some("impl EventStore for Sqlite")),
            ("Sqlite", 3, Some("impl EventStore for Sqlite")),
            ("helper", 3, Some("<Sqlite as EventStore>::append")),
            (
                "EventStore",
                4,
                Some("impl crate::store::EventStore for Kurrent"),
            ),
            (
                "Kurrent",
                4,
                Some("impl crate::store::EventStore for Kurrent"),
            ),
            ("EventStore", 5, Some("impl EventStore for Wrapper<T>")),
            ("Wrapper", 5, Some("impl EventStore for Wrapper<T>")),
            ("Sqlite", 6, Some("impl Sqlite")),
            ("open_connection", 6, Some("Sqlite::open")),
            ("open", 7, Some("open_store")),
            ("append", 7, Some("open_store")),
            ("make", 7, Some("open_store")),
        ] {
            assert!(
                refs.contains(&expected),
                "missing reference {expected:?}; got {refs:?}"
            );
        }
        assert_eq!(
            refs.iter()
                .filter(|(n, l, _)| *n == "EventStore" && *l == 3)
                .count(),
            1,
            "an impl's trait is referenced once, never twice; got {refs:?}"
        );
    }

    #[test]
    fn a_malformed_js_file_sets_partial_true_while_still_indexing_the_well_formed_part() {
        // Spec 92 criterion 2 round 4 (review REJECT `adj-u2c2-r3-verdict-reject`, finding
        // `adv-u2c2-partial-marker-unimplemented`): a JS file the grammar cannot fully parse must
        // set `FileSymbols::partial`, and its extraction must still cover as far as the parse
        // reached rather than dropping the well-formed part silently. `broken(` is left with no
        // closing paren or body, which tree-sitter recovers from via an ERROR node while still
        // parsing the earlier, well-formed `greet` function normally.
        let src = "function greet() { return 1; }\nfunction broken(\n";
        let language: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
        let fs = extract(src, Lang::Js, &language, tree_sitter_javascript::TAGS_QUERY).unwrap();
        assert!(
            fs.partial,
            "a JS source containing an unrecoverable syntax error must set partial: true; got {fs:?}"
        );
        assert!(
            fs.defs.iter().any(|d| d.name == "greet"),
            "the well-formed part of a partially-malformed file must still be indexed as far as \
             the parse reached, not dropped silently; got {:?}",
            fs.defs
        );
    }

    #[test]
    fn a_well_formed_js_file_never_sets_partial() {
        // The negative case this same criterion's Done-when requires: an ordinary, fully-parseable
        // file must NEVER carry the degraded marker.
        let src = "function greet() { return 1; }\n";
        let language: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
        let fs = extract(src, Lang::Js, &language, tree_sitter_javascript::TAGS_QUERY).unwrap();
        assert!(
            !fs.partial,
            "a well-formed JS file must never set partial: true; got {fs:?}"
        );
    }

    #[test]
    fn rust_grammar_kind_mapping_is_characterized() {
        // Drives the `kind_of` arms the UPSTREAM Rust query reaches and pins the real observed
        // lowering, so the mapping has actual coverage (not an eprintln probe).
        let src = "\
struct Widget;
enum State { On, Off }
trait Drawable { fn draw(&self); }
impl Drawable for Widget { fn draw(&self) {} }
const MAX: u8 = 9;
static GLOBAL: u8 = 1;
mod inner {}
macro_rules! mymac { () => {} }
fn free() {}
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();
        let kind_of_def = |name: &str| fs.defs.iter().find(|d| d.name == name).map(|d| d.kind);

        // The arms the upstream query reaches: struct/enum -> Type, method -> Method,
        // module -> Module, function -> Function, macro -> Macro.
        assert_eq!(kind_of_def("Widget"), Some(Kind::Type));
        assert_eq!(kind_of_def("State"), Some(Kind::Type));
        assert_eq!(kind_of_def("draw"), Some(Kind::Method));
        assert_eq!(kind_of_def("inner"), Some(Kind::Module));
        assert_eq!(kind_of_def("free"), Some(Kind::Function));
        assert_eq!(kind_of_def("mymac"), Some(Kind::Macro));

        // The upstream `tags.scm` tags a `trait` under the "interface" category, so under it
        // `Drawable` lowers to Kind::Type; the registered `RUST_TAGS` tags it a trait instead.
        assert_eq!(kind_of_def("Drawable"), Some(Kind::Type));
        // The upstream query emits no tag for a `const` or a `static`, so under it those
        // definitions are absent; `RUST_TAGS` tags both. An `impl` block is read off the parsed
        // tree instead (gap 104) and lowers to Kind::Impl, named by its header.
        assert_eq!(kind_of_def("MAX"), None);
        assert_eq!(kind_of_def("GLOBAL"), None);
        assert_eq!(kind_of_def("impl Drawable for Widget"), Some(Kind::Impl));
        // Exactly the seven tagged definitions above plus the impl block; the impl header
        // references the `Drawable` bound it implements.
        assert_eq!(fs.defs.len(), 8);
        assert!(fs.refs.iter().any(|r| r.name == "Drawable"));
    }

    #[test]
    fn a_reference_is_attributed_to_its_enclosing_definition() {
        // Spec 37 criterion 1: the extractor attributes each reference to the INNERMOST function
        // whose body encloses it (the caller), and a reference outside every definition carries
        // none. `fn f() { G(); }` yields a `SymRef` for `G` whose `enclosing` is `f`; a top-level
        // reference belonging to no definition (the `setup!()` macro call at the end) carries
        // `None`. An impl block is a definition of its own (gap 104), so the `Draw` bound in the
        // `impl Draw for Widget` header attributes to that impl.
        let src = "\
trait Draw {}
struct Widget;
impl Draw for Widget {}
fn f() {
    G();
}
fn h() {
    G();
    G();
}
fn outer() {
    fn inner() {
        G();
    }
}
setup!();
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();

        // The `Draw` bound in the `impl` header (line 3) attributes to the impl block.
        let draw = fs
            .refs
            .iter()
            .find(|r| r.name == "Draw")
            .expect("the impl-header Draw reference is extracted");
        assert_eq!(
            draw.enclosing.as_deref(),
            Some("impl Draw for Widget"),
            "an impl header's trait reference attributes to the impl it belongs to"
        );
        // The top-level `setup!()` call belongs to no definition: no caller.
        let setup = fs
            .refs
            .iter()
            .find(|r| r.name == "setup")
            .expect("the top-level setup! reference is extracted");
        assert_eq!(
            setup.enclosing, None,
            "a top-level reference outside every definition has no enclosing caller"
        );

        // The call `G()` inside `fn f` (line 5) attributes to `f`.
        let g_in_f = fs
            .refs
            .iter()
            .find(|r| r.name == "G" && r.line == 5)
            .expect("G() inside f is extracted");
        assert_eq!(
            g_in_f.enclosing.as_deref(),
            Some("f"),
            "a call inside fn f is attributed to its enclosing definition f"
        );

        // Both calls inside `fn h` (lines 8, 9) attribute to `h`.
        for line in [8, 9] {
            let g = fs
                .refs
                .iter()
                .find(|r| r.name == "G" && r.line == line)
                .unwrap_or_else(|| panic!("G() on line {line} inside h is extracted"));
            assert_eq!(
                g.enclosing.as_deref(),
                Some("h"),
                "a call inside fn h is attributed to h"
            );
        }

        // The call inside the NESTED `fn inner` (line 13) attributes to the INNERMOST function
        // `inner`, not the outer `outer` - proving innermost containment, not merely any encloser -
        // named by its path `outer::inner` (spec 119 criterion 8).
        let g_nested = fs
            .refs
            .iter()
            .find(|r| r.name == "G" && r.line == 13)
            .expect("G() inside the nested inner fn is extracted");
        assert_eq!(
            g_nested.enclosing.as_deref(),
            Some("outer::inner"),
            "a call in a nested function attributes to the innermost enclosing function"
        );
    }

    /// A REFERENCE'S CALLER IS ITS FUNCTION (spec 116 criterion 2), under `RUST_TAGS`: a
    /// file-level impl header's trait and self type are the impl's, while a fn-local impl
    /// header's, a fn-local trait body's outside its methods and a fn-local inline mod's
    /// references are the fn's; a call inside a method or fn nested in any of them stays that
    /// innermost function's; a file-level mod's and trait's own references stay theirs, no
    /// function enclosing them; and a top-level reference has the file, no caller.
    #[test]
    fn a_reference_s_caller_is_its_innermost_enclosing_function_else_definition_else_file() {
        crate::symbol_index::assert_callers(
            "rs",
            crate::caller_sources::RUST_CALLER_SOURCE,
            &[
                (3, "Draw", Some("impl Draw for Widget")),
                (3, "Widget", Some("impl Draw for Widget")),
                (3, "paint", Some("<Widget as Draw>::draw")),
                (6, "Draw", Some("host")),
                (6, "Local", Some("host")),
                (6, "brush", Some("host::<Local as Draw>::draw")),
                (7, "declare", Some("host")),
                (7, "measure", Some("host::Shape::area")),
                (8, "dig", Some("host::inner::deep")),
                (8, "setup", Some("host")),
                (10, "boot", Some("outer")),
                (11, "announce", Some("Top")),
                (12, "start", None),
            ],
        );
    }

    /// A CALLER IS NAMED BY ITS PATH (spec 119 criterion 8): a reference's `enclosing` is its
    /// caller's `Def.path`, never its leaf - a call in `impl T { fn new() }` is `T::new`'s and one
    /// in a Go receiver method `Recv::name`'s.
    #[test]
    fn a_reference_s_enclosing_is_its_caller_s_path() {
        crate::symbol_index::assert_callers(
            "rs",
            crate::caller_sources::RUST_METHOD_CALLER_SOURCE,
            &[(2, "T", Some("impl T")), (3, "helper", Some("T::new"))],
        );
        crate::symbol_index::assert_callers(
            "go",
            crate::caller_sources::GO_RECEIVER_CALLER_SOURCE,
            &[
                (5, "Recv", Some("Recv::name")),
                (5, "helper", Some("Recv::name")),
            ],
        );
    }

    /// The leaf of the caller [`enclosing_def`] picks for `pos` over hand-built
    /// `(range, leaf, kind)` definition ranges.
    fn caller(ranges: &[(std::ops::Range<usize>, &str, Kind)], pos: usize) -> Option<String> {
        let ranges: Vec<_> = ranges
            .iter()
            .map(|(range, leaf, kind)| (range.clone(), leaf.to_string(), *kind))
            .collect();
        enclosing_def(&ranges, pos).map(|i| ranges[i].1.clone())
    }

    /// THE NAME FILTER DROPS A NAMELESS DEFINITION (spec 116 criterion 3), pinned per language
    /// over the hand-built leaves empty, `_` and `x`: the empty leaf is nameless in every
    /// language, `_` only in Go and Rust, whose grammars make it the blank identifier, and `x`
    /// in none.
    #[test]
    fn the_name_filter_drops_the_empty_leaf_everywhere_and_the_blank_identifier_in_go_and_rust() {
        let cases: [(Lang, [bool; 3]); 6] = [
            (Lang::Go, [true, true, false]),
            (Lang::Rust, [true, true, false]),
            (Lang::Js, [true, false, false]),
            (Lang::Ts, [true, false, false]),
            (Lang::Python, [true, false, false]),
            (Lang::CSharp, [true, false, false]),
        ];
        for (lang, dropped) in cases {
            assert_eq!(
                ["", "_", "x"].map(|leaf| is_nameless(lang, leaf)),
                dropped,
                "{lang:?}: whether the leaves empty, `_` and `x` are dropped"
            );
        }
    }

    /// The four function-like kinds, the caller rule's first choice.
    const FUNCTION_LIKE: [Kind; 4] = [Kind::Function, Kind::Method, Kind::Constructor, Kind::Macro];

    /// Every kind, a `Declaration` apart, that is a caller only when no function-like
    /// definition encloses the reference.
    const NOT_FUNCTION_LIKE: [Kind; 9] = [
        Kind::Type,
        Kind::Trait,
        Kind::Impl,
        Kind::Module,
        Kind::Constant,
        Kind::Static,
        Kind::Variant,
        Kind::Field,
        Kind::Other,
    ];

    /// The caller filter, pinned per kind over hand-built ranges: each function-like kind
    /// enclosing a reference is its caller past an inner definition of every other kind; the
    /// innermost of two function-like definitions wins; and with no function-like definition
    /// around it, the innermost definition of any kind does.
    #[test]
    fn the_caller_filter_takes_each_function_like_kind_past_an_inner_definition() {
        for outer in FUNCTION_LIKE {
            for inner in NOT_FUNCTION_LIKE {
                assert_eq!(
                    caller(&[(10..50, "outer", outer), (20..30, "inner", inner)], 25),
                    Some("outer".to_string()),
                    "{outer:?} around {inner:?}"
                );
            }
            for inner in FUNCTION_LIKE {
                assert_eq!(
                    caller(&[(10..50, "outer", outer), (20..30, "inner", inner)], 25),
                    Some("inner".to_string()),
                    "{inner:?} inside {outer:?}"
                );
            }
        }
        for outer in NOT_FUNCTION_LIKE {
            for inner in NOT_FUNCTION_LIKE {
                assert_eq!(
                    caller(&[(10..50, "outer", outer), (20..30, "inner", inner)], 25),
                    Some("inner".to_string()),
                    "{inner:?} inside {outer:?}, no function around either"
                );
            }
        }
    }

    /// A `declaration` is no caller: a reference inside its range takes the next enclosing
    /// definition, function-like first, else the file; and a definition nested in a
    /// declaration's range is still found.
    #[test]
    fn a_declaration_is_excluded_from_the_caller_ranges() {
        let decl = (20..30, "decl", Kind::Declaration);
        assert_eq!(caller(std::slice::from_ref(&decl), 25), None);
        assert_eq!(
            caller(&[(10..50, "host", Kind::Trait), decl.clone()], 25),
            Some("host".to_string())
        );
        assert_eq!(
            caller(&[(10..50, "host", Kind::Function), decl.clone()], 25),
            Some("host".to_string())
        );
        assert_eq!(
            caller(
                &[
                    (0..60, "f", Kind::Function),
                    (10..50, "host", Kind::Trait),
                    decl.clone()
                ],
                25
            ),
            Some("f".to_string())
        );
        for inner in [Kind::Constant, Kind::Function] {
            assert_eq!(
                caller(
                    &[(20..30, "decl", Kind::Declaration), (22..28, "in", inner)],
                    25
                ),
                Some("in".to_string()),
                "{inner:?} inside a declaration's range"
            );
        }
    }

    /// Within one class of definition the smallest containing range wins, a range holding its
    /// start and not its end, equal spans breaking on the lower start and identical ranges on
    /// the lesser name; no range gives no caller.
    #[test]
    fn the_innermost_containing_range_wins_and_ties_break_on_start_then_name() {
        let one = [(10..20, "f", Kind::Function)];
        assert_eq!(caller(&one, 9), None);
        assert_eq!(caller(&one, 10), Some("f".to_string()));
        assert_eq!(caller(&one, 19), Some("f".to_string()));
        assert_eq!(caller(&one, 20), None);
        assert_eq!(caller(&[], 0), None);
        for kind in [Kind::Function, Kind::Constant] {
            assert_eq!(
                caller(&[(10..19, "outer", kind), (12..14, "inner", kind)], 13),
                Some("inner".to_string()),
                "{kind:?}: the smaller span, early in its outer"
            );
            assert_eq!(
                caller(&[(10..20, "outer", kind), (18..20, "inner", kind)], 19),
                Some("inner".to_string()),
                "{kind:?}: the smaller span, at its outer's end"
            );
            assert_eq!(
                caller(&[(15..25, "a", kind), (10..20, "z", kind)], 17),
                Some("z".to_string()),
                "{kind:?}: equal spans break on the lower start, ahead of the name"
            );
            assert_eq!(
                caller(&[(10..20, "b", kind), (10..20, "a", kind)], 15),
                Some("a".to_string()),
                "{kind:?}: identical ranges break on the lesser name"
            );
        }
    }

    // THE RUST DEFINITIONS FIXTURE (spec 116 criterion 6), the one Rust file holding every Rust
    // definition type: shared with the graph build test, which folds the same text.
    use crate::rust_definition_sources::RUST_DEFINITIONS_SOURCE;

    /// RUST DEFINITIONS ARE NODES (spec 116 criterion 6): under the registered Rust query the
    /// fixture yields every definition it holds at its own kind and leaf, an aliased
    /// `extern crate` at its alias, a tuple field at its index, a trait required fn a
    /// declaration, a proc-macro fn and a `macro_rules!` macros, and a fn and a field of one
    /// leaf each their own definition in either order; no `extern crate` is an out-of-line
    /// module; and each row of Rust's DEFINITION TYPES names exactly one of them.
    #[test]
    fn every_rust_definition_type_yields_its_definition_at_its_kind() {
        let fs = crate::symbol_index::extracted("rs", RUST_DEFINITIONS_SOURCE);
        assert_eq!(
            crate::symbol_index::defs_by_line(&fs),
            vec![
                (1, "alloc", Kind::Module),
                (2, "base", Kind::Module),
                (3, "LIMIT", Kind::Constant),
                (4, "TABLE", Kind::Static),
                (5, "0", Kind::Field),
                (5, "1", Kind::Field),
                (5, "2", Kind::Field),
                (5, "Pair", Kind::Type),
                (6, "0", Kind::Field),
                (6, "Probe", Kind::Variant),
                (6, "Shape", Kind::Type),
                (6, "Struct", Kind::Variant),
                (6, "Tuple", Kind::Variant),
                (6, "Unit", Kind::Variant),
                (6, "side", Kind::Field),
                (7, "Bits", Kind::Type),
                (7, "whole", Kind::Field),
                (8, "Alias", Kind::Type),
                (9, "Canvas", Kind::Type),
                (9, "Draw", Kind::Trait),
                (9, "paint", Kind::Declaration),
                (10, "WIDTH", Kind::Constant),
                (10, "impl Pair", Kind::Impl),
                (10, "width", Kind::Method),
                (11, "shout", Kind::Macro),
                (13, "SEED", Kind::Constant),
                (13, "derive", Kind::Macro),
                (14, "CACHE", Kind::Static),
                (14, "LOCAL", Kind::Constant),
                (14, "host", Kind::Function),
                (15, "nest", Kind::Module),
                (16, "size", Kind::Function),
                (17, "Plot", Kind::Type),
                (17, "area", Kind::Field),
                (17, "size", Kind::Field),
                (17, "spy", Kind::Field),
                (18, "area", Kind::Function),
                (19, "PROBE_LIMIT", Kind::Constant),
                (20, "PROBE_TABLE", Kind::Static),
                (21, "S", Kind::Type),
                (21, "probe", Kind::Field),
                (21, "real", Kind::Field),
                (22, "A", Kind::Variant),
                (22, "B", Kind::Variant),
                (22, "E", Kind::Type),
                (23, "impl X", Kind::Impl),
                (23, "real", Kind::Method),
                (23, "t", Kind::Method),
                (24, "f", Kind::Function),
                (25, "inner", Kind::Module),
                (25, "outer", Kind::Module),
            ]
        );
        let out_of_line: Vec<&str> = fs
            .defs
            .iter()
            .filter(|d| d.is_out_of_line_module)
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(out_of_line, Vec::<&str>::new());
        crate::symbol_index::assert_one_def_per_row(
            &fs,
            &crate::grounder::symbols::registry::tests::RUST_DEFINITION_TYPES,
        );
    }

    /// RUST DEFINITIONS ARE NODES (spec 116 criterion 6), the test attribute: a `#[cfg(test)]`
    /// tuple field, unit variant, named field, const and static are each test code, as a
    /// `#[cfg(test)]` method and mod are, and nothing else is: a struct, enum, impl, fn or mod
    /// whose body opens with a test-attributed child stays production with its other members.
    #[test]
    fn a_test_attributed_rust_member_is_test_code_and_its_owner_and_siblings_stay_production() {
        let fs = crate::symbol_index::extracted("rs", RUST_DEFINITIONS_SOURCE);
        assert_eq!(
            crate::symbol_index::test_code_by_line(&fs).0,
            vec![
                (5, "2"),
                (6, "Probe"),
                (17, "spy"),
                (19, "PROBE_LIMIT"),
                (20, "PROBE_TABLE"),
                (21, "probe"),
                (22, "A"),
                (23, "t"),
                (25, "inner"),
            ]
        );
    }

    /// RUST DEFINITIONS ARE NODES (spec 116 criterion 6), the callers: a call in an item
    /// static's initializer is the static's, no function enclosing it, while a call in a const
    /// local to a fn is the fn's and one in a const local to a proc-macro fn the macro's; an impl
    /// header's self type stays the impl's.
    #[test]
    fn a_call_in_a_rust_data_definition_is_its_function_s_else_the_definition_s() {
        crate::symbol_index::assert_callers(
            "rs",
            RUST_DEFINITIONS_SOURCE,
            &[
                (4, "build", Some("TABLE")),
                (10, "Pair", Some("impl Pair")),
                (13, "seed", Some("derive")),
                (14, "compute", Some("host")),
                (23, "X", Some("impl X")),
            ],
        );
    }

    /// A fn under any of the three proc-macro attributes is a macro, wherever the attribute
    /// sits in the fn's attribute stack; a fn under another attribute stays a function, and a
    /// proc-macro attribute on an item that is no fn changes nothing.
    #[test]
    fn a_fn_under_a_proc_macro_attribute_is_a_macro() {
        let src = "\
#[proc_macro_derive(Shape)]
pub fn derive_shape() {}
#[proc_macro_attribute]
pub fn route() {}
#[inline]
fn plain() {}
#[proc_macro]
struct Marked;
/// Expands a call.
#[proc_macro]
// A comment inside the stack.
#[allow(dead_code)]
pub fn bang() {}
";
        let fs = crate::symbol_index::extracted("rs", src);
        assert_eq!(
            crate::symbol_index::defs_by_line(&fs),
            vec![
                (2, "derive_shape", Kind::Macro),
                (4, "route", Kind::Macro),
                (6, "plain", Kind::Function),
                (8, "Marked", Kind::Type),
                (13, "bang", Kind::Macro),
            ]
        );
    }

    crate::test_cases! {
        /// The Rust read-offs are selected by the language `extract` is given: the fixture read
        /// under the Rust grammar and query as another language makes no tuple field and leaves
        /// the proc-macro fn a function, while what the query tags stays.
        the_rust_read_offs_run_only_for_the_rust_language:
            assert_defs_read_as(
                "rs",
                RUST_DEFINITIONS_SOURCE,
                Lang::Go,
                &[5, 13],
                &[
                    (5, "Pair", Kind::Type),
                    (13, "SEED", Kind::Constant),
                    (13, "derive", Kind::Function),
                ],
            );
        /// The Go read-offs are selected by the language `extract` is given: the fixture read
        /// under the Go grammar and query as another language makes no embedded field and no
        /// alias, while what the query tags stays.
        the_go_read_offs_run_only_for_the_go_language:
            assert_defs_read_as(
                "go",
                crate::go_definition_sources::GO_DEFINITIONS_SOURCE,
                Lang::Rust,
                &[8, 9],
                &[
                    (8, "Shape", Kind::Type),
                    (8, "side", Kind::Field),
                    (8, "x", Kind::Field),
                    (8, "y", Kind::Field),
                ],
            );
        /// The C# read-offs are selected by the language `extract` is given: the fixture read
        /// under the C# grammar and query as another language makes no operator, conversion
        /// operator or indexer, while what the query tags stays, the destructor at the leaf
        /// its node gives it.
        the_csharp_read_offs_run_only_for_the_csharp_language:
            assert_defs_read_as(
                "cs",
                crate::csharp_definition_sources::CSHARP_DEFINITIONS_SOURCE,
                Lang::Rust,
                &[13, 14, 15, 16],
                &[(13, "~Foo", Kind::Method)],
            );
    }

    /// `src`, read under the grammar and query registered for `ext` as the language `lang`,
    /// holds exactly the definitions `expected` on `lines`.
    fn assert_defs_read_as(
        ext: &str,
        src: &str,
        lang: Lang,
        lines: &[u32],
        expected: &[(u32, &str, Kind)],
    ) {
        let entry = crate::symbol_index::registered(ext);
        let fs = extract(src, lang, &entry.language, entry.tags_query).unwrap();
        let on_lines: Vec<_> = crate::symbol_index::defs_by_line(&fs)
            .into_iter()
            .filter(|(line, _, _)| lines.contains(line))
            .collect();
        assert_eq!(on_lines, expected);
    }

    /// The attribute walk starts below the root: a file that is one mod, its range the root's
    /// own, still reads the inner attribute its body opens with, so the mod and what it holds
    /// are test code.
    #[test]
    fn a_definition_spanning_the_whole_file_is_still_read_for_its_inner_test_attribute() {
        let fs = crate::symbol_index::extracted("rs", "mod t { #![cfg(test)] fn h() {} }");
        assert_eq!(
            crate::symbol_index::test_code_by_line(&fs).0,
            vec![(1, "h"), (1, "t")]
        );
    }

    crate::test_cases! {
        /// A TEST REGION IS EVERY TEST-ATTRIBUTED RANGE (spec 116 criterion 11): a
        /// `#[cfg(test)] const _` the name filter drops makes the fn it holds and that fn's
        /// reference test, while an unattributed `const _`'s fn and a `#[cfg(unix)]` fn stay
        /// production.
        a_test_attributed_definition_the_name_filter_drops_makes_what_it_holds_test:
            assert_test_code(
                "\
#[cfg(test)]
const _: () = {
    fn helper() {
        real();
    }
};
const _: () = {
    fn kept() {
        real();
    }
};
fn real() {}
#[cfg(unix)]
fn f() {}
",
                &[(3, "helper")],
                &[(4, "real")],
            );
        /// A TEST REGION IS EVERY TEST-ATTRIBUTED RANGE (spec 116 criterion 11): a
        /// `#[cfg(test)] extern "C"` block, which yields no definition of its own, makes the
        /// fn it holds test, while an unattributed extern block's fn stays production.
        a_test_attributed_extern_block_makes_what_it_holds_test:
            assert_test_code(
                "\
#[cfg(test)]
extern \"C\" {
    fn f();
}
extern \"C\" {
    fn g();
}
",
                &[(3, "f")],
                &[],
            );
        /// A TEST REGION IS EVERY TEST-ATTRIBUTED RANGE (spec 116 criterion 11): a file opening
        /// `#![cfg(test)]` is test code whole, its second fn, its `mod x;` and every reference
        /// included, not only the item after the attribute.
        a_file_opening_an_inner_cfg_test_is_test_code_whole:
            assert_test_code(
                "\
#![cfg(test)]
fn first() {}
fn second() {
    first();
}
mod x;
",
                &[(2, "first"), (3, "second"), (6, "x")],
                &[(4, "first")],
            );
        /// A file opening an OUTER `#[cfg(test)] use x;` is no test region: the attribute
        /// gates the `use` alone, so the fns after it and their references stay production.
        a_file_opening_a_test_attributed_use_keeps_its_fns_production:
            assert_test_code(
                "\
#[cfg(test)]
use x;
fn a() {}
fn b() {
    a();
}
",
                &[],
                &[],
            );
        /// A file opening an OUTER `#[cfg(test)] mod tests;` is no test region: the attribute
        /// makes the `mod` test alone, so the fns after it and their references stay production.
        a_file_opening_a_test_attributed_mod_declaration_keeps_its_fns_production:
            assert_test_code(
                "\
#[cfg(test)]
mod tests;
fn a() {}
fn b() {
    a();
}
",
                &[(2, "tests")],
                &[],
            );
    }

    /// The Rust source `src`, read through the registered grammar and query, marks exactly the
    /// definitions `defs` and the references `refs` test code, each as `(line, name)`.
    fn assert_test_code(src: &str, defs: &[(u32, &str)], refs: &[(u32, &str)]) {
        let fs = crate::symbol_index::extracted("rs", src);
        assert_eq!(
            crate::symbol_index::test_code_by_line(&fs),
            (defs.to_vec(), refs.to_vec())
        );
    }

    crate::test_cases! {
        /// `is_declaration` is consulted for a function, method or constructor only: a
        /// signature node an injected query tags as another kind keeps that kind.
        a_signature_node_tagged_as_no_function_keeps_its_tagged_kind:
            assert_required_fn_lowers("constant", Kind::Constant);
        /// `is_declaration` is consulted for a method as for a function: a signature node an
        /// injected query tags a method is a declaration.
        a_signature_node_tagged_a_method_is_a_declaration:
            assert_required_fn_lowers("method", Kind::Declaration);
    }

    /// A trait's one required fn, `m`, lowers to exactly one definition, of `kind`, under an
    /// injected query that tags its signature node a definition of the tag category `category`.
    fn assert_required_fn_lowers(category: &str, kind: Kind) {
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(
            "trait T { fn m(&self); }\n",
            Lang::Rust,
            &language,
            &format!("(function_signature_item name: (identifier) @name) @definition.{category}"),
        )
        .unwrap();
        assert_eq!(crate::symbol_index::defs_by_line(&fs), vec![(1, "m", kind)]);
    }

    /// The symbols the Rust grammar extracts from `src`.
    fn rust_symbols(src: &str) -> FileSymbols {
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap()
    }

    /// Each `(name, is_test, why)` of `expected` names a definition of the Rust source `src` and
    /// whether it is marked test code; returns the extracted symbols.
    fn assert_defs_marked_test(src: &str, expected: &[(&str, bool, &str)]) -> FileSymbols {
        let fs = rust_symbols(src);
        for (name, is_test, why) in expected {
            let def = fs
                .defs
                .iter()
                .find(|d| d.name == *name)
                .unwrap_or_else(|| panic!("no def named {name:?}; got {:?}", fs.defs));
            assert_eq!(def.is_test, *is_test, "{name}: {why}");
        }
        fs
    }

    #[test]
    fn test_annotated_definitions_and_everything_nested_inside_them_are_marked_is_test() {
        // Spec 86 criterion 1's own fixture shape: product code alongside a `#[cfg(test)]` module
        // that itself holds a plain, UNATTRIBUTED helper and a `#[test]` function. A `#[test]`
        // function, a `#[cfg(test)]` module, and everything - definitions AND references - nested
        // inside either, are marked `is_test`; product code (and a reference PRODUCT code makes)
        // stays `is_test: false`.
        let src = "\
fn product() {
    helper_call();
}

#[cfg(test)]
mod tests {
    // A plain helper with NO attribute of its own - still test code, by CONTAINMENT inside the
    // cfg(test) module, not by its own annotation.
    fn helper() {
        product();
    }

    #[test]
    fn it_works() {
        helper();
    }
}
";
        let fs = assert_defs_marked_test(
            src,
            &[
                ("product", false, "product code is never test code"),
                ("tests", true, "the #[cfg(test)] module itself is test code"),
                (
                    "helper",
                    true,
                    "a plain helper with no attribute of its own is test code by CONTAINMENT \
                     inside the cfg(test) module",
                ),
                ("it_works", true, "a #[test] function is test code"),
            ],
        );

        // References: the call inside product code is not test code; the calls inside the
        // (transitively) test-scoped `helper`/`it_works` bodies are.
        let ref_is_test = |name: &str, line: u32| {
            fs.refs
                .iter()
                .find(|r| r.name == name && r.line == line)
                .unwrap_or_else(|| panic!("no ref {name:?}@{line}; got {:?}", fs.refs))
                .is_test
        };
        assert!(
            !ref_is_test("helper_call", 2),
            "a reference from product code is not test code"
        );
        assert!(
            ref_is_test("product", 10),
            "a reference from the unattributed helper nested in cfg(test) is test code"
        );
        assert!(
            ref_is_test("helper", 15),
            "a reference from the #[test] function is test code"
        );
    }

    crate::test_cases! {
        /// `#[cfg(all(test, feature = "x"))]` is the exact shape this repository's own source
        /// uses (e.g. a `#[cfg(all(test, feature = "symbols"))] mod tests` gate) - a compound
        /// predicate naming `test` as one of several conjuncts must still be recognized.
        /// Conversely a similarly-spelled but DISTINCT token (`testing`) must never
        /// false-positive: `test` is matched as a whole word, never a substring.
        cfg_predicates_naming_test_are_recognized_and_similarly_spelled_tokens_are_not:
            assert_defs_marked_test(
                "\
#[cfg(all(test, feature = \"x\"))]
fn compound_predicate() {}

#[cfg(feature = \"testing\")]
fn similarly_spelled_feature_is_not_a_test() {}

#[allow(dead_code)]
#[cfg(test)]
fn a_stacked_non_test_attribute_above_does_not_hide_the_real_one() {}
",
                &[
                    (
                        "compound_predicate",
                        true,
                        "#[cfg(all(test, ..))] names the test token as one conjunct and must match",
                    ),
                    (
                        "similarly_spelled_feature_is_not_a_test",
                        false,
                        "\"testing\" is a distinct token from \"test\" and must never false-positive",
                    ),
                    (
                        "a_stacked_non_test_attribute_above_does_not_hide_the_real_one",
                        true,
                        "every attribute in the stack is inspected, not just the one nearest the item",
                    ),
                ],
            );
        /// Round-2 regression (review REJECT adj-u86c1-verdict-reject / adv-u86c1-cfg-predicate-
        /// negation-and-cfg-attr-inverted): the token scan named `test` as a standalone word with
        /// zero cfg-predicate structure, so it wrongly folded two common, ALWAYS-product Rust
        /// idioms to `is_test: true`. `#[cfg(not(test))]` is the PRODUCTION-only half of a
        /// dual-cfg mock construct: the item it guards is definitionally the one that SHIPS
        /// (compiled whenever `test` is NOT set). `cfg_attr` never gates compilation of the
        /// tagged item itself - it only conditionally attaches the inner attribute - so the item
        /// is ALWAYS compiled regardless of what its predicate names.
        negated_and_cfg_attr_predicates_naming_test_do_not_mark_the_item_test:
            assert_defs_marked_test(
                "\
#[cfg(not(test))]
fn real_client() {}

#[cfg_attr(test, derive(Debug))]
struct RealConfig;
",
                &[
                    (
                        "real_client",
                        false,
                        "#[cfg(not(test))] guards the production half of a dual-cfg construct - \
                         never test code",
                    ),
                    (
                        "RealConfig",
                        false,
                        "cfg_attr's predicate governs the inner attribute, not the tagged item's \
                         own compilation",
                    ),
                ],
            );
        /// Round-6 mutation testing finding: `predicate_group_facts`'s `not` arm used to (in an
        /// earlier version of this fix) invert its inner predicate's answer UNCONDITIONALLY -
        /// sound only when the inner predicate is built purely from `test` (a
        /// `not(test)`/`not(not(test))`/... chain), but wrongly also applied to a `not(P)`
        /// wrapping an UNRELATED atom. `feature = "x"`'s own truth is independent of `test`
        /// altogether (on or off regardless of `test`'s value), so `not(feature = "x")` compiles
        /// whenever `feature` is OFF - in EITHER a test or a non-test build - and must never be
        /// marked test code, exactly as an item under an ordinary, un-negated
        /// `#[cfg(feature = "x")]` never is.
        a_not_wrapping_a_non_test_atom_never_marks_the_item_test: assert_defs_marked_test(
            "\
#[cfg(not(feature = \"x\"))]
fn real_client() {}

#[cfg(not(not(test)))]
fn double_negation_is_test_only() {}
",
            &[
                (
                    "real_client",
                    false,
                    "not(feature = \"x\") is independent of test altogether - never test code",
                ),
                (
                    "double_negation_is_test_only",
                    true,
                    "not(not(test)) cancels back to test-only - a PURE test predicate, so \
                     inversion is sound here and must still recognize it as test code",
                ),
            ],
        );
        /// Round-3 regression (review REJECT adj-u86c1-verdict-reject round 2, findings
        /// arch-u86c1-r2-compound-not-predicate-still-marks-product-code-test and
        /// sdet-u86c1-r2-cfg-predicate-fix-does-not-generalize-to-nested-negation-or-any): round
        /// 2's fix only special-cased a `not(..)` wrapping the WHOLE predicate. A `not(test)`
        /// nested as a sub-clause of `all(..)`, or a `test` disjunct sitting alongside a non-test
        /// one inside `any(..)`, both fell through the flat token scan and were wrongly folded to
        /// `is_test: true`, excluding real product items from the graph.
        compound_predicates_with_nested_negation_or_a_non_test_disjunct_do_not_mark_the_item_test:
            assert_defs_marked_test(
                "\
#[cfg(all(not(test), feature = \"x\"))]
fn dual_cfg_mock_production_half() {}

#[cfg(any(debug_assertions, test))]
fn debug_only_helper_that_ships_in_every_non_release_build() {}
",
                &[
                    (
                        "dual_cfg_mock_production_half",
                        false,
                        "not(test) nested inside all(..) still means the item compiles whenever \
                         test is NOT set - the production half of a dual-cfg construct, never \
                         test code",
                    ),
                    (
                        "debug_only_helper_that_ships_in_every_non_release_build",
                        false,
                        "any(debug_assertions, test) gives the item a path to compile with test \
                         unset - it is not test-only even though the predicate names the test \
                         token",
                    ),
                ],
            );
        /// Round-3 regression (review REJECT adj-u86c1-verdict-reject round 2, finding
        /// adv-u86c1-r2-trailing-comment-severs-the-attribute-stack-scan): a same-line `//`
        /// comment after the attribute's closing `]` made `strip_suffix(']')` fail on the whole
        /// line, hit the else-arm `break`, and stopped the upward scan immediately - so a
        /// `#[cfg(test)]` module with a trailing same-line comment on its own attribute was never
        /// recognized as self-attributed test, and the whole module (and everything nested inside
        /// it) leaked into the graph as ordinary product code.
        a_trailing_same_line_comment_on_a_cfg_test_attribute_does_not_sever_the_scan:
            assert_defs_marked_test(
                "\
fn product() {}

#[cfg(test)] // module gate, trailing comment
mod tests {
    fn helper() {}

    #[test]
    fn it_works() {}
}
",
                &[
                    ("product", false, "product code is never test code"),
                    (
                        "tests",
                        true,
                        "the #[cfg(test)] module is still recognized as self-attributed test \
                         despite the trailing same-line comment on its attribute",
                    ),
                    (
                        "helper",
                        true,
                        "an unattributed helper nested inside the trailing-commented cfg(test) \
                         module is test code by containment",
                    ),
                    ("it_works", true, "a #[test] function is test code"),
                ],
            );
        /// Round-5 regression (review REJECT adj-u86c1-verdict-reject round 4, finding
        /// adv-u86c1-r4-inner-cfg-test-attribute-not-recognized): `preceded_by_test_attribute`'s
        /// sibling walk matched only the OUTER `attribute_item` shape (`#[cfg(test)] mod tests {
        /// .. }`, the attribute a sibling BEFORE the mod). The equally idiomatic INNER-attribute
        /// form - `mod tests { #![cfg(test)] fn helper() {} }`, where the attribute is the FIRST
        /// node INSIDE the mod's own body instead - parses to a distinct grammar kind,
        /// `inner_attribute_item` (verified against the real parsed tree: `mod_item body:
        /// declaration_list(inner_attribute_item, function_item)`), which the match fell through
        /// to its `_ => break` arm on, silently treating both the mod and its plain nested helper
        /// as ordinary product code.
        ///
        /// Two distinct gaps, both closed here: (1) `helper` is a normal SIBLING of the inner
        /// attribute inside the mod's declaration_list, so it is caught the same way an
        /// outer-attributed item's sibling stack already was - `inner_attribute_item` added to the
        /// existing sibling-walk match arm. (2) the mod ITSELF has no preceding sibling at all (the
        /// attribute lives inside its own body, not before it), so the sibling walk alone can never
        /// self-attribute it; `leading_inner_test_attribute` closes this by checking whether the
        /// definition's own body OPENS with a test-naming inner attribute, mirroring Rust's actual
        /// semantic (an inner attribute governs the item whose body contains it).
        an_inner_cfg_test_attribute_marks_its_enclosing_module_and_the_module_marks_its_children:
            assert_defs_marked_test(
                "\
mod tests {
    #![cfg(test)]

    // A plain helper with NO attribute of its own - still test code, both by its own inner-
    // attribute-preceded sibling position AND by containment inside the now-self-attributed mod.
    fn helper() {
        product();
    }
}

fn product() {}
",
                &[
                    (
                        "tests",
                        true,
                        "a mod whose OWN body opens with #![cfg(test)] is self-attributed test \
                         code, exactly as the equivalent outer #[cfg(test)] mod form already is",
                    ),
                    (
                        "helper",
                        true,
                        "a plain helper with no attribute of its own, sitting inside a \
                         #![cfg(test)]-gated mod's body, is test code - by direct sibling \
                         position and by containment alike",
                    ),
                    ("product", false, "product code is never test code"),
                ],
            );
    }

    #[test]
    fn comma_separated_groups_keeps_a_nested_parenthesized_comma_grouped_and_splits_only_the_outer_one(
    ) {
        // Structural analogue of the round 1-3 text scan's own depth-tracking property, now
        // proven against a real parsed tree rather than a hand-rolled character loop: a comma
        // nested one level deeper than the group being split (`all(x, y)`'s own internal comma,
        // itself already isolated into a nested `token_tree` child by the grammar) must never
        // count as a split point for the OUTER group - only the comma directly among the outer
        // group's own children does. Reached by parsing `#[cfg(any(all(x, y), z))]` and reading
        // `any`'s own argument list, `"(all(x, y), z)"` - the exact shape `predicate_group_names_
        // test`'s `all`/`any` case recurses into via this same function.
        let src = "#[cfg(any(all(x, y), z))]\nfn f() {}\n";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let root = tree.root_node();
        let attribute_item = root.named_child(0).expect("the leading attribute_item");
        let attribute = attribute_item
            .named_child(0)
            .expect("attribute_item's sole named child is the attribute node");
        let cfg_args = attribute
            .child_by_field_name("arguments")
            .expect("cfg's own parenthesized arguments");
        // cfg always has exactly one top-level group: [identifier("any"), token_tree(the rest)].
        let any_group = comma_separated_groups(cfg_args)
            .into_iter()
            .next()
            .expect("cfg(..) has exactly one top-level group");
        let any_args = any_group[1];
        assert_eq!(any_args.kind(), "token_tree", "any's own argument list");

        let groups = comma_separated_groups(any_args);
        assert_eq!(
            groups.len(),
            2,
            "\"all(x, y), z\" splits into exactly 2 top-level groups; the comma nested inside \
             all(x, y)'s own parens must never count as a split point, got {groups:?}"
        );
        assert_eq!(groups[0][0].utf8_text(src.as_bytes()).unwrap(), "all");
        assert_eq!(groups[1][0].utf8_text(src.as_bytes()).unwrap(), "z");
    }

    #[test]
    fn malformed_tags_query_surfaces_as_err_not_panic() {
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        // A malformed tags query must return Err from the config step, never panic, so a bad
        // registry entry degrades loudly instead of crashing the indexer.
        let bad = extract("fn f(){}", Lang::Rust, &language, "(this is not valid scm");
        assert!(bad.is_err(), "malformed query should be Err, got {bad:?}");
        let msg = bad.unwrap_err();
        assert!(
            msg.starts_with("symbols: tags config:"),
            "error should come from the tags-config step, got: {msg}"
        );
        // The happy path over the SAME grammar still succeeds - the Err above is the query, not
        // the language: `f` is extracted as a function definition.
        let ok = extract(
            "fn f(){}",
            Lang::Rust,
            &language,
            tree_sitter_rust::TAGS_QUERY,
        )
        .expect("valid query extracts");
        assert!(ok
            .defs
            .iter()
            .any(|d| d.name == "f" && d.kind == Kind::Function));
        // The name-slice guard (`source.get(..)` -> `continue`) is a defensive arm the tags
        // mechanism never triggers for valid UTF-8 boundaries; it is exercised for its Some
        // side by every extraction here and by `extracts_a_rust_definition_and_a_reference`.
    }

    /// The `(line, end_line)` of the one definition named `name` the grammar and query registered
    /// for the extension `ext` extract from `source`, failing loudly when there is none, so an
    /// extent assertion never silently passes on a miss.
    fn extent_of(ext: &str, source: &str, name: &str) -> (u32, u32) {
        let symbols = crate::symbol_index::extracted(ext, source);
        crate::symbol_index::extents_by_line(&symbols)
            .into_iter()
            .find(|(_, _, extent_name)| *extent_name == name)
            .map(|(line, end_line, _)| (line, end_line))
            .unwrap_or_else(|| panic!("no definition named {name:?} in the extracted source"))
    }

    /// Each `(name, line, end_line, why)` of `expected` is the extent the extension `ext`'s
    /// registered grammar and query give that definition of `source`.
    fn assert_extents(ext: &str, source: &str, expected: &[(&str, u32, u32, &str)]) {
        for (name, line, end_line, why) in expected {
            assert_eq!(
                extent_of(ext, source, name),
                (*line, *end_line),
                "{name}: {why}"
            );
        }
    }

    crate::test_cases! {
        /// The exact criterion-1 OUTPUT defect the hand-rolled brace lexer re-triggered
        /// (adv-u58c1-signature-brace-early-close): a `{` ON the signature line - a struct
        /// destructuring parameter, or an `= {}` default - opens and closes a brace on that line,
        /// so a lexer that counts the FIRST `{` truncates the body to the signature alone. The
        /// grammar's OWN node range spans the whole function, so the extent covers the full body.
        extent_spans_a_destructuring_or_default_brace_signature_to_the_full_body: {
            // `config` on line 1 destructures a `Point { x, y }` param (a brace ON the
            // signature); its body runs to the closing brace on line 4.
            assert_extents(
                "rs",
                "\
fn config(Point { x, y }: Point) -> u32 {
    let sum = x + y;
    sum
}
",
                &[(
                    "config",
                    1,
                    4,
                    "the extent spans the FULL body past the signature brace, not the signature \
                     line alone",
                )],
            );
            // The `= {}` default-argument shape of the same defect: a brace that opens and closes
            // on the signature line still must not bound the body.
            assert_extents(
                "rs",
                "\
fn with_default(opts: Opts) {
    let empty = Opts {};
    use_it(empty);
}
",
                &[(
                    "with_default",
                    1,
                    4,
                    "an `= {}`/`{}` literal in the body never early-closes the extent",
                )],
            );
        };
        /// The multi-grammar face (adv-u58c1-multigrammar-mislex-confirmed): the extent authority
        /// is the grammar's own node range, so it is correct on the BRACELESS and
        /// brace-carrying-string grammars a Rust brace lexer mis-reads. Two ingested grammars
        /// beyond Rust.
        extent_generalizes_across_grammars_python_nested_def_and_js_brace_string: {
            // (a) Python (braceless): a nested `def` inside an outer `def`. A Rust lexer finds no
            // brace and falls back to the next-def-by-line, truncating the outer body at its
            // nested child; Python's block boundary is the dedent, which the grammar resolves.
            assert_extents(
                "py",
                "\
def outer(n):
    def inner(k):
        return k + 1
    total = inner(n)
    return total
",
                &[
                    (
                        "outer",
                        1,
                        5,
                        "the Python outer def spans its whole indented block past the nested \
                         inner def",
                    ),
                    (
                        "inner",
                        2,
                        3,
                        "the nested Python def is bounded by its own dedent, not the outer's tail",
                    ),
                ],
            );
            // (b) JS: a function whose body holds a single-quote string carrying a lone `{`. A
            // Rust lexer that does not treat a single-quote as a string delimiter counts that `{`
            // as a body open and over-reads into the NEXT function; the grammar knows the quote
            // is a string. The following function is a DISTINCT extent, proving `open` did not
            // swallow it.
            assert_extents(
                "js",
                "\
function open() {
    const brace = '{';
    return brace;
}
function next() {
    return 2;
}
",
                &[
                    (
                        "open",
                        1,
                        4,
                        "the JS body's single-quote `{` never over-reads the extent into the \
                         next function",
                    ),
                    ("next", 5, 7, "the following function is its own extent"),
                ],
            );
        };
    }

    #[test]
    fn extent_of_a_one_line_definition_is_a_single_line() {
        // A one-line body: its line and end line coincide. Guards the end-byte-to-line mapping on
        // the smallest construct.
        assert_eq!(extent_of("rs", "fn tiny() {}\n", "tiny"), (1, 1));
    }

    /// The `end_line` [`def_of`] gives a definition at `line` spanning `range` of `text`.
    fn end_line_of(text: &str, range: std::ops::Range<usize>, line: u32) -> u32 {
        def_of(&Source::new(text), range, Kind::Function, "f".into(), line).end_line
    }

    crate::test_cases! {
        /// A range ends on its last byte's line: `ab\ncd` ends on line 2.
        a_range_ends_on_the_line_of_its_last_byte: assert_eq!(end_line_of("ab\ncd\nef", 0..5, 1), 2);
        /// A range whose last byte is a newline ends on the line that newline closes, never the
        /// next one.
        a_range_ending_with_a_newline_ends_on_the_line_it_closes:
            assert_eq!(end_line_of("ab\ncd\n", 0..3, 1), 1);
        /// A zero-length range gives `end_line` = `line`, though the byte before it sits on a
        /// later line.
        a_zero_length_range_ends_on_its_own_line: assert_eq!(end_line_of("a\nb\nc\n", 5..5, 1), 1);
        /// An empty source gives `end_line` = `line`.
        an_empty_source_ends_on_its_own_line: assert_eq!(end_line_of("", 0..0, 3), 3);
        /// A range whose last byte sits above `line` ends on `line`, never below it.
        a_range_ending_above_its_line_ends_on_its_line: assert_eq!(end_line_of("a\nb\nc\n", 0..1, 3), 3);
    }

    crate::test_cases! {
        /// Rust: the tagged `tagged` ends on its closing brace and its `line` is its name row, below
        /// its attribute and its visibility; the read-off tuple fields `0` and `1` end on their own
        /// type's line; the impl block ends on its closing brace and its method `get` on its own
        /// line.
        every_rust_definition_ends_on_its_constructs_last_line: assert_eq!(
            crate::symbol_index::extents_by_line(&crate::symbol_index::extracted(
                "rs", "\
#[derive(Debug)]
pub struct Pair(u8, u16);

#[inline]
pub(crate)
fn tagged() {
    let _ = 1;
}

impl Pair {
    fn get(&self) {}
}
",
            )),
            [
                (2, 2, "0"),
                (2, 2, "1"),
                (2, 2, "Pair"),
                (6, 8, "tagged"),
                (10, 12, "impl Pair"),
                (11, 11, "get")
            ]
        );
        /// Go: the tagged `S` and `Run` end on their closing braces; the read-off embedded field
        /// `Base` ends on its own line.
        every_go_definition_ends_on_its_constructs_last_line: assert_eq!(
            crate::symbol_index::extents_by_line(&crate::symbol_index::extracted(
                "go", "\
package p

type S struct {
	Base
	n int
}

func Run() {
}
",
            )),
            [(3, 6, "S"), (4, 4, "Base"), (5, 5, "n"), (8, 9, "Run")]
        );
        /// Python: the decorated `tagged`'s `line` is its name row below its decorator and it ends
        /// on its block's last line; the read-off class-body name `slot` ends on its own line.
        every_python_definition_ends_on_its_constructs_last_line: assert_eq!(
            crate::symbol_index::extents_by_line(&crate::symbol_index::extracted(
                "py", "\
@decorator
def tagged():
    return 1

class K:
    slot: int
",
            )),
            [(2, 3, "tagged"), (5, 6, "K"), (6, 6, "slot")]
        );
        /// JS: the tagged class `C` and method `m` end on their closing braces; the read-off class
        /// field `f` ends on its own line.
        every_js_definition_ends_on_its_constructs_last_line: assert_eq!(
            crate::symbol_index::extents_by_line(&crate::symbol_index::extracted(
                "js", "\
class C {
  f = 1;
  m() {
    return 2;
  }
}
",
            )),
            [(1, 6, "C"), (2, 2, "f"), (3, 5, "m")]
        );
        /// TS: the decorated class `D`'s `line` is its name row below its decorator; the tagged
        /// enum `E` ends on its closing brace and its read-off member `A` on its own line.
        every_ts_definition_ends_on_its_constructs_last_line: assert_eq!(
            crate::symbol_index::extents_by_line(&crate::symbol_index::extracted(
                "ts", "\
@sealed
class D {}
enum E {
  A,
}
",
            )),
            [(2, 2, "D"), (3, 5, "E"), (4, 4, "A")]
        );
        /// C#: the attributed method `M`'s `line` is its name row below its attribute and its
        /// return type; the tagged class `K` ends on its closing brace and the read-off operator
        /// `+` on its own line.
        every_csharp_definition_ends_on_its_constructs_last_line: assert_eq!(
            crate::symbol_index::extents_by_line(&crate::symbol_index::extracted(
                "cs", "\
class K
{
    [Obsolete]
    public void
    M() { }
    public static K operator +(K a, K b) => a;
}
",
            )),
            [(1, 7, "K"), (5, 5, "M"), (6, 6, "+")]
        );
    }

    /// Where [`locate_extent`] bounds the Rust definition named `name` of kind `kind` recorded at
    /// `line` in `source`, at no path, under the registered Rust grammar and query.
    fn located(source: &str, name: &str, kind: Kind, line: u32) -> Extent {
        let entry = crate::symbol_index::registered("rs");
        locate_extent(
            source,
            entry.lang,
            &entry.language,
            entry.tags_query,
            Recorded {
                name,
                path: None,
                kind: kind_str(kind),
                line,
            },
        )
        .expect("the registered query runs over the source")
    }

    crate::test_cases! {
        /// Tier 1: two definitions of the recorded name and kind at the recorded line bound by the
        /// wider.
        two_tier_one_matches_bound_by_the_wider: assert_eq!(
            located("fn a() {} fn a() {\n}\n", "a", Kind::Function, 1),
            Extent::Bounded { start: 1, end: 2 }
        );
        /// Tier 1 reads the kind: a variant inside its same-named enum, both on the recorded line,
        /// bounds to the variant, never the wider enum.
        a_variant_inside_its_same_named_enum_bounds_to_the_variant: assert_eq!(
            located("enum V { V {\n    a: u8,\n},\n}\n", "V", Kind::Variant, 1),
            Extent::Bounded { start: 1, end: 3 }
        );
        /// Tier 2: a definition whose kind changed at its recorded name and line bounds there,
        /// beside a same-named definition of the recorded kind elsewhere in the file.
        a_definition_whose_kind_changed_in_place_bounds_at_its_recorded_line: assert_eq!(
            located(crate::extent_sources::KIND_CHANGED_IN_PLACE_SOURCE, "f", Kind::Function, 1),
            Extent::Bounded { start: 1, end: 2 }
        );
        /// Tier 2 takes exactly one: two same-named definitions of other kinds at the recorded line
        /// fall to tier 3, which heals to the one of the recorded kind.
        two_other_kinds_at_the_recorded_line_fall_to_the_third_tier: assert_eq!(
            located("struct g {} const g: u8 = 0;\nfn g() {\n}\n", "g", Kind::Function, 1),
            Extent::Bounded { start: 2, end: 3 }
        );
        /// Tier 3: a drifted fn beside a same-named field heals to the fn, the field never making
        /// the name ambiguous.
        a_drifted_fn_beside_a_same_named_field_heals: assert_eq!(
            located(crate::extent_sources::FN_BESIDE_SAME_NAMED_FIELD_SOURCE, "h", Kind::Function, 1),
            Extent::Bounded { start: 5, end: 6 }
        );
        /// Tier 3 reads production definitions only: a drifted fn beside its `#[cfg(test)]` twin
        /// heals to the production one.
        a_drifted_production_fn_beside_a_cfg_test_twin_bounds_to_the_production_one: assert_eq!(
            located("\nfn x() {\n}\n#[cfg(test)]\nfn x() {}\n", "x", Kind::Function, 1),
            Extent::Bounded { start: 2, end: 3 }
        );
        /// Tier 3 refuses an ambiguous name: two drifted definitions of the name and kind at two
        /// lines are left unbounded.
        an_ambiguous_drifted_name_is_left_unbounded: assert_eq!(
            located("fn other() {}\nfn dup() {}\nfn dup() {}\n", "dup", Kind::Function, 1),
            Extent::Ambiguous(2)
        );
        /// Two same-named definitions of other kinds at the recorded line, and none of the recorded
        /// kind anywhere, are missing: tier 2 takes exactly one, tier 3 finds no line.
        two_other_kinds_at_the_recorded_line_and_none_of_its_kind_are_missing: assert_eq!(
            located(crate::extent_sources::TWO_OTHER_KINDS_AT_ONE_LINE_SOURCE, "g", Kind::Function, 1),
            Extent::Missing
        );
        /// A name no definition carries anywhere in the file is missing.
        a_name_defined_nowhere_is_missing: assert_eq!(
            located("fn other() {}\n", "gone", Kind::Function, 1),
            Extent::Missing
        );
    }

    /// Spec 119, IDS FOLLOW THE LANGUAGE'S PATH: two definitions with no byte between them, the
    /// second starting where the first ends, are siblings at file level, neither one holding the
    /// other although each holds one end of the other's range.
    #[test]
    fn adjacent_definitions_are_siblings_never_each_others_container() {
        let symbols = crate::symbol_index::extracted("rs", "fn a(){}fn b(){}\n");
        let paths: Vec<&str> = symbols.defs.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["a", "b"], "adjacent definitions sit side by side");
    }
}
