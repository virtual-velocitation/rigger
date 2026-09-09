//! Symbol extraction: the ONE function that touches tree-sitter (architecture 5.5.3). It
//! runs an INJECTED grammar's `tags` query over a source string and lowers the uniform tags
//! into the parser-free `FileSymbols` model. Everything downstream sees `FileSymbols` and
//! never the parser - so per-language support is a registration (unit 2), not bespoke code.

use crate::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef};
use tree_sitter_tags::{TagsConfiguration, TagsContext};

/// Map a grammar tag's syntax-type NAME (the name half of a `tags.scm` category, e.g.
/// "function", "method", "struct", "class", "interface", "trait", "module", "constant") to a
/// rigger `Kind`. Unknown names fold to `Other`, so the model stays grammar-agnostic and a new
/// grammar never forces a `Kind` variant.
fn kind_of(syntax_type: &str) -> Kind {
    match syntax_type {
        "function" => Kind::Function,
        "method" => Kind::Method,
        "struct" | "class" | "enum" | "type" | "interface" => Kind::Type,
        "trait" => Kind::Trait,
        "impl" => Kind::Impl,
        "module" | "namespace" => Kind::Module,
        "constant" => Kind::Constant,
        _ => Kind::Other,
    }
}

/// Extract one file's definitions and references by running the INJECTED grammar's tag query
/// over `source` (5.5.3). This is the ONLY function that touches tree-sitter; the caller
/// supplies the grammar and its query (unit 2's registry decides WHICH), and everything it
/// returns is the parser-free `FileSymbols`.
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
    let mut defs = Vec::new();
    let mut refs = Vec::new();
    // Each definition's byte range (the whole construct, body included - the tag's node range,
    // see `enclosing_def`) paired with its name, and each reference's byte position. Collected in
    // this single tag pass; the enclosing definition is resolved below so the reference order the
    // emit pass depends on stays untouched.
    let mut def_ranges: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut ref_positions: Vec<usize> = Vec::new();
    for tag in tags {
        let tag = tag.map_err(|e| format!("symbols: tag: {e}"))?;
        let Some(name) = source.get(tag.name_range.start..tag.name_range.end) else {
            continue;
        };
        let name = name.to_string();
        // 1-based line from the tag span's start row.
        let line = tag.span.start.row as u32 + 1;
        if tag.is_definition {
            let syntax = config.syntax_type_name(tag.syntax_type_id);
            def_ranges.push((tag.range.clone(), name.clone()));
            defs.push(Def {
                kind: kind_of(syntax),
                name,
                line,
                // Resolved below, once every definition's range is known (test_regions needs the
                // WHOLE set to decide containment, not just what has been seen so far).
                is_test: false,
                // Resolved below too, from the second parsed tree (only a `Module`-kind def can
                // ever be true here; see `Def::is_out_of_line_module`).
                is_out_of_line_module: false,
                path_override: None,
            });
        } else {
            ref_positions.push(tag.range.start);
            refs.push(SymRef {
                name,
                line,
                enclosing: None,
                is_test: false,
            });
        }
    }
    // Attribute each reference to the innermost enclosing definition (the caller, spec 37). The
    // reference order is unchanged - `enclosing` is a derived per-reference attribute, never a new
    // sort key, so identical source still yields byte-identical downstream events.
    for (r, &pos) in refs.iter_mut().zip(ref_positions.iter()) {
        r.enclosing = enclosing_def(&def_ranges, pos);
    }
    // Spec 86 criterion 1: mark every definition and reference that falls inside a TEST REGION -
    // a definition directly annotated `#[test]`/`#[cfg(test)]` (or nested inside one). Byte-range
    // based, so it is exact regardless of which line a construct starts or ends on; computed
    // AFTER every def's range is known, so a def's own containment check can see siblings and
    // ancestors alike whatever order the tags happened to arrive in.
    //
    // Round 4 (review REJECT `adj-u86c1-verdict-reject` round 3, findings
    // `sdet-u86c1-r3-embedded-slash-attribute-plus-trailing-comment-severs-scan` /
    // `arch-u86c1-r3-recurring-scan-defects-are-a-structural-parser-gap`): the 6th recurrence of a
    // hand-rolled text scan of `#[...]` attribute shape. `test_regions` now reads the SAME parsed
    // tree `tags_query` runs against - a second, full-grammar `tree_sitter::Parser::parse` over
    // this same `source` - and walks its `attribute_item` nodes structurally instead of
    // re-deriving attribute/comment boundaries from characters; this is still the one function
    // touching tree-sitter, so the single-parsing-authority invariant (5.5.3) survives. A
    // `Parser::set_language` or `parse` failure here is defensive only: `ts_language` already
    // parsed successfully above via `TagsConfiguration::new`/`generate_tags`, using the identical
    // language, so this path degrades to an `Err` rather than a panic without ever being expected
    // to fire in practice.
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(ts_language)
        .map_err(|e| format!("symbols: parser language: {e}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "symbols: parse: tree-sitter produced no syntax tree".to_string())?;
    // Round 6 (`op-u86c1-r5-close-every-remaining-test-shape` item 3): a self-attributed
    // `#[cfg(test)] impl Widget { .. }` is a test-region container exactly like a self-attributed
    // `mod_item` already is, but `impl_item` is never itself a `def_ranges` entry (tags.scm tags it
    // only as `@reference.implementation`, never a `@definition.*` - see
    // `self_attributed_impl_regions`'s doc), so `test_regions` alone can never find it. Found by a
    // direct tree walk instead and merged into the SAME `regions` list, so the containment check
    // below covers a plain method nested in such an impl by the identical mechanism that already
    // covers a plain helper nested in a `#[cfg(test)] mod`.
    let mut regions = test_regions(source.as_bytes(), tree.root_node(), &def_ranges);
    regions.extend(self_attributed_impl_regions(
        tree.root_node(),
        source.as_bytes(),
    ));
    if !regions.is_empty() {
        for (d, (range, _)) in defs.iter_mut().zip(def_ranges.iter()) {
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
    for (d, (range, _)) in defs.iter_mut().zip(def_ranges.iter()) {
        if d.kind == Kind::Module {
            if let Some(n) = tree
                .root_node()
                .descendant_for_byte_range(range.start, range.end)
            {
                d.is_out_of_line_module =
                    n.kind() == "mod_item" && n.child_by_field_name("body").is_none();
                if d.is_out_of_line_module {
                    d.path_override = out_of_line_path_override(n, source.as_bytes());
                }
            }
        }
    }
    Ok(FileSymbols { lang, defs, refs })
}

/// The byte ranges of every SELF-ATTRIBUTED test definition in `def_ranges` - one whose own
/// `#[test]`/`#[cfg(..test..)]` attribute stack sits directly above it in the parsed tree rooted
/// at `root` ([`preceded_by_test_attribute`]). A definition NESTED inside one of these ranges (a
/// plain helper with no attribute of its own, living inside a `#[cfg(test)] mod tests { .. }`) is
/// test code too, but it earns that status by CONTAINMENT, not by appearing in this list - the
/// caller checks containment against the ranges this returns, so a doubly-nested item is covered
/// by the SAME outer range without this function needing to recurse.
fn test_regions(
    source: &[u8],
    root: tree_sitter::Node,
    def_ranges: &[(std::ops::Range<usize>, String)],
) -> Vec<std::ops::Range<usize>> {
    def_ranges
        .iter()
        .filter(|(range, _)| preceded_by_test_attribute(source, root, range))
        .map(|(range, _)| range.clone())
        .collect()
}

/// Whether the definition spanning `range` is directly preceded - skipping only comment nodes
/// (`line_comment`, which covers plain `//` and doc `///`/`//!` alike, and `block_comment`) - by
/// an attribute node that GATES THE ITEM'S OWN COMPILATION on `test`: bare `#[test]`, or
/// `#[cfg(...)]` whose predicate names `test` ([`attribute_item_names_test`] /
/// [`cfg_predicate_token_tree_names_test`]). `range` is looked up in the ALREADY-PARSED tree
/// rooted at `root` ([`root.descendant_for_byte_range`]) - the SAME node the tags query captured,
/// since a tag's range is exactly its underlying grammar node's range - and the scan walks that
/// node's `prev_sibling()` chain: an `attribute_item` (or comment) is a sibling of the item it
/// decorates in every grammar shape this crate has ever seen it in (a top-level item, a `mod`
/// body, an `impl` body), so this generalizes to any nesting depth without a separate case for
/// each. The walk stops at the first sibling that is neither a comment nor an `attribute_item` -
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
/// the SAME shared walk ([`attribute_stack_names_test`]) in opposite directions over different
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
/// Round 6: the actual test - both directions - is [`node_preceded_by_test_attribute`], factored
/// out so [`self_attributed_impl_regions`] can ask it of a node it already has in hand (from a
/// direct tree walk) without a redundant `descendant_for_byte_range` round-trip through a range
/// that was never in `def_ranges` to begin with (`impl_item` never is - see that function's doc).
fn preceded_by_test_attribute(
    source: &[u8],
    root: tree_sitter::Node,
    range: &std::ops::Range<usize>,
) -> bool {
    let Some(node) = root.descendant_for_byte_range(range.start, range.end) else {
        return false;
    };
    node_preceded_by_test_attribute(node, source)
}

/// The shared core [`preceded_by_test_attribute`] and [`self_attributed_impl_regions`] both apply
/// once a candidate node is in hand: self-attributed test either through the node's OWN leading
/// inner attribute ([`leading_inner_test_attribute`]) or through an outer attribute stack sitting
/// as its sibling immediately before it.
fn node_preceded_by_test_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    leading_inner_test_attribute(node, source)
        || attribute_stack_names_test(source, node.prev_sibling(), |n| n.prev_sibling())
}

/// Round 6 (`op-u86c1-r5-close-every-remaining-test-shape` item 3): every self-attributed
/// `impl_item` node's own byte range, found by walking the parsed tree DIRECTLY rather than through
/// `def_ranges` the way [`test_regions`] finds a self-attributed `mod_item`/`function_item`/etc. -
/// `tree-sitter-rust`'s own `tags.scm` captures `impl_item` ONLY as `@reference.implementation`,
/// never as any `@definition.*`, so an impl block can never appear in `def_ranges` in the first
/// place and `test_regions`'s filter-over-`def_ranges` can structurally never see it, no matter how
/// it is attributed. The caller merges these ranges into the SAME `regions` list `test_regions`
/// returns, so a plain method (or any other item) nested inside a self-attributed impl block is
/// marked test BY CONTAINMENT - the identical mechanism that already covers a plain helper nested
/// inside a self-attributed `#[cfg(test)] mod`, with no separate containment rule needed here.
fn self_attributed_impl_regions(
    root: tree_sitter::Node,
    source: &[u8],
) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    collect_self_attributed_impl_regions(root, source, &mut out);
    out
}

/// The preorder tree walk behind [`self_attributed_impl_regions`]: every `impl_item` node anywhere
/// under `node` (not merely at the top level - a `#[cfg(test)] impl` can itself sit inside another
/// container) that [`node_preceded_by_test_attribute`]s contributes its own `byte_range()`.
fn collect_self_attributed_impl_regions(
    node: tree_sitter::Node,
    source: &[u8],
    out: &mut Vec<std::ops::Range<usize>>,
) {
    if node.kind() == "impl_item" && node_preceded_by_test_attribute(node, source) {
        out.push(node.byte_range());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_self_attributed_impl_regions(child, source, out);
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
/// a `const_item` that has no body to open one in) and walks forward from its first named child via
/// the SAME [`attribute_stack_names_test`] the sibling-before case uses, so a doc comment or a
/// stacked `#![allow(..)]` ahead of the real `#![cfg(test)]` never hides it either.
fn leading_inner_test_attribute(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = node.child_by_field_name("body") else {
        return false;
    };
    attribute_stack_names_test(source, body.named_child(0), |n| n.next_named_sibling())
}

/// The one walk shared by both [`preceded_by_test_attribute`] (backward over `prev_sibling`) and
/// [`leading_inner_test_attribute`] (forward over `next_named_sibling`, from a body's first named
/// child), and (round 7) [`out_of_line_path_override`]: starting at `first`, follow `advance`
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
/// question [`preceded_by_test_attribute`]/[`leading_inner_test_attribute`] both need -
/// [`attribute_item_names_test`] is the `visit` closure, and presence-of-a-match (`Some(())`)
/// collapses to the boolean this and every caller actually wants.
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

/// Whether a parsed `attribute_item` node is a `#[path = "value"]` key-value attribute - the ONLY
/// shape `path` ever takes (unlike `cfg`'s parenthesized `arguments`, `path` uses the grammar's
/// `value` field; see `tree-sitter-rust`'s own `node-types.json`, `attribute`'s `value` field) -
/// and if so, its string literal's content with the surrounding quote characters stripped. `None`
/// for every other attribute name, and for a `path` attribute whose value is not a plain
/// double-quoted string literal (round-7 scope: rustc itself requires one here, so this is
/// defensive only, never expected to fire on real source).
fn path_attribute_value(item: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let attribute = item.named_child(0)?;
    let name_node = attribute.named_child(0)?;
    if name_node.utf8_text(source).unwrap_or_default() != "path" {
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
    let Some(attribute) = item.named_child(0) else {
        return false;
    };
    let Some(name_node) = attribute.named_child(0) else {
        return false;
    };
    match name_node.utf8_text(source).unwrap_or_default() {
        "test" => true,
        "cfg" => attribute
            .child_by_field_name("arguments")
            .is_some_and(|args| cfg_predicate_token_tree_names_test(args, source)),
        _ => false,
    }
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
/// never as two separate walks over the identical shape (a round-6 `cargo mutants` finding /
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
///   in EITHER a test or a non-test build, so it is definitively NOT test-only (a round-6 `cargo
///   mutants` finding: reading an earlier, un-guarded `!inner_names_test` as a general "P is not
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

/// The name of the INNERMOST definition whose byte range contains `pos`, or `None` when `pos`
/// lies outside every definition (a top-level reference such as an import or an `impl`-header
/// bound). Each definition tag carries the byte range of the WHOLE tagged construct - the function
/// body included, not just its name span - so a reference inside a body falls within its
/// definition's range. "Innermost" is the smallest containing range, so a reference in a nested
/// definition attributes to the nested one, not its outer scope. Deterministic for identical
/// input: ties on span break on the range start, then the definition name.
fn enclosing_def(def_ranges: &[(std::ops::Range<usize>, String)], pos: usize) -> Option<String> {
    def_ranges
        .iter()
        .filter(|(range, _)| range.contains(&pos))
        .min_by(|(a, a_name), (b, b_name)| {
            (a.end - a.start)
                .cmp(&(b.end - b.start))
                .then_with(|| a.start.cmp(&b.start))
                .then_with(|| a_name.cmp(b_name))
        })
        .map(|(_, name)| name.clone())
}

/// A definition's 1-based inclusive LINE extent: its site line and the last line of the WHOLE
/// tagged construct (its body included). Both are derived from the tree-sitter tag range, so the
/// end is the grammar's OWN node boundary - the closing brace of a braced language, the dedent of a
/// Python block, the closing backtick of a Go raw string - never a hand-rolled per-language guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefExtent {
    /// The definition name (the same name the `<file>::<name>` graph id carries).
    pub name: String,
    /// The 1-based site line: the definition's name row - IDENTICAL to the `line` [`Def`] records
    /// and the graph persists, so a located entity's recorded line matches this extent's start.
    pub start_line: u32,
    /// The 1-based, inclusive line the whole construct (body included) ends on - the last line of
    /// the tree-sitter node range. Always `>= start_line`.
    pub end_line: u32,
}

/// The 1-based line extent of every definition in `source`, under the INJECTED grammar (the SAME
/// `(grammar, tag query)` pair [`extract`] runs and the registry supplies). This is the ONE
/// multi-grammar extent authority the show surface bounds a body with: because the end line is read
/// from the grammar's own tree-sitter node range, it generalizes across every ingested grammar
/// (Rust, Python, Go, JS/TS, C#) instead of a Rust-only brace lexer - so a destructuring parameter
/// (`fn f(Point { x, y }: Point) {`), a Python nested `def`, a Go backtick raw string, and a JS
/// single-quote string carrying a lone `{` are all bounded by the parser, never a lexer that
/// mis-reads them. It shares the tag mechanism ([`tree_sitter_tags`]) with `extract`; it projects
/// the node's END line rather than lowering the persisted `Def`, so it records nothing and changes
/// no serialized form. A definition whose byte range cannot be mapped is skipped (the same
/// defensive guard `extract` uses), never a panic.
pub fn definition_extents(
    source: &str,
    ts_language: &tree_sitter::Language,
    tags_query: &str,
) -> Result<Vec<DefExtent>, String> {
    let config = TagsConfiguration::new(ts_language.clone(), tags_query, "")
        .map_err(|e| format!("symbols: tags config: {e}"))?;
    let mut ctx = TagsContext::new();
    let (tags, _) = ctx
        .generate_tags(&config, source.as_bytes(), None)
        .map_err(|e| format!("symbols: generate tags: {e}"))?;
    // Byte offset at which each 0-based line begins, so a construct's end byte maps to its end line
    // by one binary search rather than rescanning the source per definition.
    let line_starts = line_start_offsets(source);
    let mut out = Vec::new();
    for tag in tags {
        let tag = tag.map_err(|e| format!("symbols: tag: {e}"))?;
        if !tag.is_definition {
            continue;
        }
        let Some(name) = source.get(tag.name_range.start..tag.name_range.end) else {
            continue;
        };
        // The site line is the name row, exactly as `extract` records it into `Def::line`.
        let start_line = tag.span.start.row as u32 + 1;
        // The construct's LAST byte (the tag range is exclusive at its end); its line is the extent
        // end. Empty source or a zero-length range degrades to `start_line` via the `.max` below.
        let last_byte = tag
            .range
            .end
            .saturating_sub(1)
            .min(source.len().saturating_sub(1));
        let end_line = (line_of_byte(&line_starts, last_byte) + 1).max(start_line);
        out.push(DefExtent {
            name: name.to_string(),
            start_line,
            end_line,
        });
    }
    Ok(out)
}

/// The byte offset at which each 0-based line begins. `offsets[0]` is always `0`; each `\n` opens
/// the next line at the byte after it. Used to map a tree-sitter node's end byte to its end line.
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

    #[test]
    fn rust_grammar_kind_mapping_is_characterized() {
        // Drives every reachable `kind_of` arm through the ONLY shipped grammar and pins the
        // real observed lowering, so the mapping has actual coverage (not an eprintln probe).
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

        // Reachable arms with the Rust grammar: struct/enum -> Type, method -> Method,
        // module -> Module, function -> Function, macro (unknown category) -> Other.
        assert_eq!(kind_of_def("Widget"), Some(Kind::Type));
        assert_eq!(kind_of_def("State"), Some(Kind::Type));
        assert_eq!(kind_of_def("draw"), Some(Kind::Method));
        assert_eq!(kind_of_def("inner"), Some(Kind::Module));
        assert_eq!(kind_of_def("free"), Some(Kind::Function));
        assert_eq!(kind_of_def("mymac"), Some(Kind::Other));

        // KNOWN grammar limitation (NON-BLOCKING; a unit-2 tag-query concern, characterized
        // here rather than changed in unit 1): the Rust `tags.scm` tags a `trait` under the
        // "interface" category, so `Drawable` lowers to Kind::Type, NOT Kind::Trait - i.e.
        // Kind::Trait is an unreachable arm with the only shipped grammar today.
        assert_eq!(kind_of_def("Drawable"), Some(Kind::Type));
        // The Rust tags query emits no tag for an `impl` block, a `const`, or a `static`, so
        // those definitions are absent (Kind::Impl / Kind::Constant are likewise unreachable
        // with this grammar - the const/static drop is the same unit-2 tag-query concern).
        assert_eq!(kind_of_def("MAX"), None);
        assert_eq!(kind_of_def("GLOBAL"), None);
        assert_eq!(kind_of_def("impl"), None);
        // Exactly the seven tagged definitions above; the lone reference is the `Drawable`
        // bound named in the `impl` header.
        assert_eq!(fs.defs.len(), 7);
        assert!(fs.refs.iter().any(|r| r.name == "Drawable"));
    }

    #[test]
    fn a_reference_is_attributed_to_its_enclosing_definition() {
        // Spec 37 criterion 1: the extractor attributes each reference to the INNERMOST definition
        // whose body encloses it (the caller), and a reference outside every definition carries
        // none. `fn f() { G(); }` yields a `SymRef` for `G` whose `enclosing` is `f`; a top-level
        // reference belonging to no function body carries `None`. (The Rust tags query captures an
        // `impl`-header trait bound as a reference but not a plain `use` import, so the top-level
        // no-caller case here is the `impl Draw for Widget` header's `Draw` bound - a faithful
        // realization of the spec's "a reference not inside any definition carries none".)
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
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();

        // The `Draw` bound in the `impl` header (line 3) belongs to no function body: no caller.
        let draw = fs
            .refs
            .iter()
            .find(|r| r.name == "Draw")
            .expect("the impl-header Draw reference is extracted");
        assert_eq!(
            draw.enclosing, None,
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

        // The call inside the NESTED `fn inner` (line 13) attributes to the INNERMOST definition
        // `inner`, not the outer `outer` - proving innermost containment, not merely any encloser.
        let g_nested = fs
            .refs
            .iter()
            .find(|r| r.name == "G" && r.line == 13)
            .expect("G() inside the nested inner fn is extracted");
        assert_eq!(
            g_nested.enclosing.as_deref(),
            Some("inner"),
            "a call in a nested definition attributes to the innermost enclosing definition"
        );
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
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();

        let def_is_test = |name: &str| {
            fs.defs
                .iter()
                .find(|d| d.name == name)
                .unwrap_or_else(|| panic!("no def named {name:?}; got {:?}", fs.defs))
                .is_test
        };
        assert!(!def_is_test("product"), "product code is never test code");
        assert!(
            def_is_test("tests"),
            "the #[cfg(test)] module itself is test code"
        );
        assert!(
            def_is_test("helper"),
            "a plain helper with no attribute of its own is test code by CONTAINMENT inside the \
             cfg(test) module"
        );
        assert!(def_is_test("it_works"), "a #[test] function is test code");

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

    #[test]
    fn cfg_predicates_naming_test_are_recognized_and_similarly_spelled_tokens_are_not() {
        // `#[cfg(all(test, feature = "x"))]` is the exact shape this repository's own source uses
        // (e.g. a `#[cfg(all(test, feature = "symbols"))] mod tests` gate) - a compound predicate
        // naming `test` as one of several conjuncts must still be recognized. Conversely a
        // similarly-spelled but DISTINCT token (`testing`) must never false-positive: `test` is
        // matched as a whole word, never a substring.
        let src = "\
#[cfg(all(test, feature = \"x\"))]
fn compound_predicate() {}

#[cfg(feature = \"testing\")]
fn similarly_spelled_feature_is_not_a_test() {}

#[allow(dead_code)]
#[cfg(test)]
fn a_stacked_non_test_attribute_above_does_not_hide_the_real_one() {}
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();
        let def_is_test = |name: &str| {
            fs.defs
                .iter()
                .find(|d| d.name == name)
                .unwrap_or_else(|| panic!("no def named {name:?}; got {:?}", fs.defs))
                .is_test
        };
        assert!(
            def_is_test("compound_predicate"),
            "#[cfg(all(test, ..))] names the test token as one conjunct and must match"
        );
        assert!(
            !def_is_test("similarly_spelled_feature_is_not_a_test"),
            "\"testing\" is a distinct token from \"test\" and must never false-positive"
        );
        assert!(
            def_is_test("a_stacked_non_test_attribute_above_does_not_hide_the_real_one"),
            "every attribute in the stack is inspected, not just the one nearest the item"
        );
    }

    #[test]
    fn negated_and_cfg_attr_predicates_naming_test_do_not_mark_the_item_test() {
        // Round-2 regression (review REJECT adj-u86c1-verdict-reject / adv-u86c1-cfg-predicate-
        // negation-and-cfg-attr-inverted): the token scan named `test` as a standalone word with
        // zero cfg-predicate structure, so it wrongly folded two common, ALWAYS-product Rust
        // idioms to `is_test: true`.
        let src = "\
#[cfg(not(test))]
fn real_client() {}

#[cfg_attr(test, derive(Debug))]
struct RealConfig;
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();
        let def_is_test = |name: &str| {
            fs.defs
                .iter()
                .find(|d| d.name == name)
                .unwrap_or_else(|| panic!("no def named {name:?}; got {:?}", fs.defs))
                .is_test
        };
        // `#[cfg(not(test))]` is the PRODUCTION-only half of a dual-cfg mock construct: the item
        // it guards is definitionally the one that SHIPS (compiled whenever `test` is NOT set),
        // never test code, so it must never be excluded from the graph.
        assert!(
            !def_is_test("real_client"),
            "#[cfg(not(test))] guards the production half of a dual-cfg construct - never test code"
        );
        // `cfg_attr` never gates compilation of the tagged item itself - it only conditionally
        // attaches the inner attribute - so the item is ALWAYS compiled regardless of what its
        // predicate names, and must never be excluded on that predicate's account.
        assert!(
            !def_is_test("RealConfig"),
            "cfg_attr's predicate governs the inner attribute, not the tagged item's own compilation"
        );
    }

    #[test]
    fn a_not_wrapping_a_non_test_atom_never_marks_the_item_test() {
        // Round-6 `cargo mutants` finding: `predicate_group_facts`'s `not` arm used to (in an
        // earlier version of this fix) invert its inner predicate's answer UNCONDITIONALLY -
        // sound only when the inner
        // predicate is built purely from `test` (a `not(test)`/`not(not(test))`/... chain), but
        // wrongly also applied to a `not(P)` wrapping an UNRELATED atom. `feature = "x"`'s own
        // truth is independent of `test` altogether (on or off regardless of `test`'s value), so
        // `not(feature = "x")` compiles whenever `feature` is OFF - in EITHER a test or a
        // non-test build - and must never be marked test code, exactly as an item under an
        // ordinary, un-negated `#[cfg(feature = "x")]` never is.
        let src = "\
#[cfg(not(feature = \"x\"))]
fn real_client() {}

#[cfg(not(not(test)))]
fn double_negation_is_test_only() {}
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();
        let def_is_test = |name: &str| {
            fs.defs
                .iter()
                .find(|d| d.name == name)
                .unwrap_or_else(|| panic!("no def named {name:?}; got {:?}", fs.defs))
                .is_test
        };
        assert!(
            !def_is_test("real_client"),
            "not(feature = \"x\") is independent of test altogether - never test code"
        );
        assert!(
            def_is_test("double_negation_is_test_only"),
            "not(not(test)) cancels back to test-only - a PURE test predicate, so inversion is \
             sound here and must still recognize it as test code"
        );
    }

    #[test]
    fn compound_predicates_with_nested_negation_or_a_non_test_disjunct_do_not_mark_the_item_test() {
        // Round-3 regression (review REJECT adj-u86c1-verdict-reject round 2, findings
        // arch-u86c1-r2-compound-not-predicate-still-marks-product-code-test and
        // sdet-u86c1-r2-cfg-predicate-fix-does-not-generalize-to-nested-negation-or-any):
        // round 2's fix only special-cased a `not(..)` wrapping the WHOLE predicate. A
        // `not(test)` nested as a sub-clause of `all(..)`, or a `test` disjunct sitting
        // alongside a non-test one inside `any(..)`, both fell through the flat token scan
        // and were wrongly folded to `is_test: true`, excluding real product items from the
        // graph.
        let src = "\
#[cfg(all(not(test), feature = \"x\"))]
fn dual_cfg_mock_production_half() {}

#[cfg(any(debug_assertions, test))]
fn debug_only_helper_that_ships_in_every_non_release_build() {}
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();
        let def_is_test = |name: &str| {
            fs.defs
                .iter()
                .find(|d| d.name == name)
                .unwrap_or_else(|| panic!("no def named {name:?}; got {:?}", fs.defs))
                .is_test
        };
        assert!(
            !def_is_test("dual_cfg_mock_production_half"),
            "not(test) nested inside all(..) still means the item compiles whenever test is \
             NOT set - the production half of a dual-cfg construct, never test code"
        );
        assert!(
            !def_is_test("debug_only_helper_that_ships_in_every_non_release_build"),
            "any(debug_assertions, test) gives the item a path to compile with test unset - it \
             is not test-only even though the predicate names the test token"
        );
    }

    #[test]
    fn a_trailing_same_line_comment_on_a_cfg_test_attribute_does_not_sever_the_scan() {
        // Round-3 regression (review REJECT adj-u86c1-verdict-reject round 2, finding
        // adv-u86c1-r2-trailing-comment-severs-the-attribute-stack-scan): a same-line `//`
        // comment after the attribute's closing `]` made `strip_suffix(']')` fail on the
        // whole line, hit the else-arm `break`, and stopped the upward scan immediately - so
        // a `#[cfg(test)]` module with a trailing same-line comment on its own attribute was
        // never recognized as self-attributed test, and the whole module (and everything
        // nested inside it) leaked into the graph as ordinary product code.
        let src = "\
fn product() {}

#[cfg(test)] // module gate, trailing comment
mod tests {
    fn helper() {}

    #[test]
    fn it_works() {}
}
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();
        let def_is_test = |name: &str| {
            fs.defs
                .iter()
                .find(|d| d.name == name)
                .unwrap_or_else(|| panic!("no def named {name:?}; got {:?}", fs.defs))
                .is_test
        };
        assert!(!def_is_test("product"), "product code is never test code");
        assert!(
            def_is_test("tests"),
            "the #[cfg(test)] module is still recognized as self-attributed test despite the \
             trailing same-line comment on its attribute"
        );
        assert!(
            def_is_test("helper"),
            "an unattributed helper nested inside the trailing-commented cfg(test) module is \
             test code by containment"
        );
        assert!(def_is_test("it_works"), "a #[test] function is test code");
    }

    #[test]
    fn an_inner_cfg_test_attribute_marks_its_enclosing_module_and_the_module_marks_its_children() {
        // Round-5 regression (review REJECT adj-u86c1-verdict-reject round 4, finding
        // adv-u86c1-r4-inner-cfg-test-attribute-not-recognized): `preceded_by_test_attribute`'s
        // sibling walk matched only the OUTER `attribute_item` shape (`#[cfg(test)] mod tests {
        // .. }`, the attribute a sibling BEFORE the mod). The equally idiomatic INNER-attribute
        // form - `mod tests { #![cfg(test)] fn helper() {} }`, where the attribute is the FIRST
        // node INSIDE the mod's own body instead - parses to a distinct grammar kind,
        // `inner_attribute_item` (verified against the real parsed tree: `mod_item body:
        // declaration_list(inner_attribute_item, function_item)`), which the match fell through
        // to its `_ => break` arm on, silently treating both the mod and its plain nested helper
        // as ordinary product code.
        //
        // Two distinct gaps, both closed here: (1) `helper` is a normal SIBLING of the inner
        // attribute inside the mod's declaration_list, so it is caught the same way an
        // outer-attributed item's sibling stack already was - `inner_attribute_item` added to the
        // existing sibling-walk match arm. (2) the mod ITSELF has no preceding sibling at all (the
        // attribute lives inside its own body, not before it), so the sibling walk alone can never
        // self-attribute it; `leading_inner_test_attribute` closes this by checking whether the
        // definition's own body OPENS with a test-naming inner attribute, mirroring Rust's actual
        // semantic (an inner attribute governs the item whose body contains it).
        let src = "\
mod tests {
    #![cfg(test)]

    // A plain helper with NO attribute of its own - still test code, both by its own inner-
    // attribute-preceded sibling position AND by containment inside the now-self-attributed mod.
    fn helper() {
        product();
    }
}

fn product() {}
";
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let fs = extract(src, Lang::Rust, &language, tree_sitter_rust::TAGS_QUERY).unwrap();
        let def_is_test = |name: &str| {
            fs.defs
                .iter()
                .find(|d| d.name == name)
                .unwrap_or_else(|| panic!("no def named {name:?}; got {:?}", fs.defs))
                .is_test
        };
        assert!(
            def_is_test("tests"),
            "a mod whose OWN body opens with #![cfg(test)] is self-attributed test code, exactly \
             as the equivalent outer #[cfg(test)] mod form already is"
        );
        assert!(
            def_is_test("helper"),
            "a plain helper with no attribute of its own, sitting inside a #![cfg(test)]-gated \
             mod's body, is test code - by direct sibling position and by containment alike"
        );
        assert!(!def_is_test("product"), "product code is never test code");
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

    /// Fetch the single extent named `name` from a `(source, grammar, query)` run, failing loudly
    /// if the grammar did not tag it - so an extent assertion never silently passes on a miss.
    fn extent_of(
        source: &str,
        ts_language: &tree_sitter::Language,
        tags_query: &str,
        name: &str,
    ) -> DefExtent {
        definition_extents(source, ts_language, tags_query)
            .unwrap_or_else(|e| panic!("definition_extents failed: {e}"))
            .into_iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("no extent named {name:?} in the tagged source"))
    }

    #[test]
    fn extent_spans_a_destructuring_or_default_brace_signature_to_the_full_body() {
        // The exact criterion-1 OUTPUT defect the hand-rolled brace lexer re-triggered
        // (adv-u58c1-signature-brace-early-close): a `{` ON the signature line - a struct
        // destructuring parameter, or an `= {}` default - opens and closes a brace on that line, so
        // a lexer that counts the FIRST `{` truncates the body to the signature alone. The grammar's
        // OWN node range spans the whole function, so the extent covers the full body.
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        // `config` on line 1 destructures a `Point { x, y }` param (a brace ON the signature); its
        // body runs to the closing brace on line 4.
        let destructure = "\
fn config(Point { x, y }: Point) -> u32 {
    let sum = x + y;
    sum
}
";
        let e = extent_of(
            destructure,
            &language,
            tree_sitter_rust::TAGS_QUERY,
            "config",
        );
        assert_eq!(e.start_line, 1, "the site line is the signature line");
        assert_eq!(
            e.end_line, 4,
            "the extent spans the FULL body past the signature brace, not the signature line alone"
        );

        // The `= {}` default-argument shape of the same defect: a brace that opens and closes on the
        // signature line still must not bound the body.
        let default_body = "\
fn with_default(opts: Opts) {
    let empty = Opts {};
    use_it(empty);
}
";
        let d = extent_of(
            default_body,
            &language,
            tree_sitter_rust::TAGS_QUERY,
            "with_default",
        );
        assert_eq!(d.start_line, 1);
        assert_eq!(
            d.end_line, 4,
            "an `= {{}}`/`{{}}` literal in the body never early-closes the extent"
        );
    }

    #[test]
    fn extent_generalizes_across_grammars_python_nested_def_and_js_brace_string() {
        // The multi-grammar face (adv-u58c1-multigrammar-mislex-confirmed): the extent authority is
        // the grammar's own node range, so it is correct on the BRACELESS and brace-carrying-string
        // grammars a Rust brace lexer mis-reads. Two ingested grammars beyond Rust:

        // (a) Python (braceless): a nested `def` inside an outer `def`. A Rust lexer finds no brace
        // and falls back to the next-def-by-line, truncating the outer body at its nested child;
        // Python's block boundary is the dedent, which the grammar resolves.
        let py_language: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
        let python = "\
def outer(n):
    def inner(k):
        return k + 1
    total = inner(n)
    return total
";
        let outer = extent_of(
            python,
            &py_language,
            tree_sitter_python::TAGS_QUERY,
            "outer",
        );
        assert_eq!(outer.start_line, 1);
        assert_eq!(
            outer.end_line, 5,
            "the Python outer def spans its whole indented block past the nested inner def"
        );
        let inner = extent_of(
            python,
            &py_language,
            tree_sitter_python::TAGS_QUERY,
            "inner",
        );
        assert_eq!(inner.start_line, 2);
        assert_eq!(
            inner.end_line, 3,
            "the nested Python def is bounded by its own dedent, not the outer's tail"
        );

        // (b) JS: a function whose body holds a single-quote string carrying a lone `{`. A Rust
        // lexer that does not treat a single-quote as a string delimiter counts that `{` as a body
        // open and over-reads into the NEXT function; the grammar knows the quote is a string.
        let js_language: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
        let js = "\
function open() {
    const brace = '{';
    return brace;
}
function next() {
    return 2;
}
";
        let open = extent_of(js, &js_language, tree_sitter_javascript::TAGS_QUERY, "open");
        assert_eq!(open.start_line, 1);
        assert_eq!(
            open.end_line, 4,
            "the JS body's single-quote `{{` never over-reads the extent into the next function"
        );
        // The following function is a DISTINCT extent, proving `open` did not swallow it.
        let next = extent_of(js, &js_language, tree_sitter_javascript::TAGS_QUERY, "next");
        assert_eq!(next.start_line, 5);
        assert_eq!(next.end_line, 7);
    }

    #[test]
    fn extent_of_a_one_line_definition_is_a_single_line() {
        // A one-line body: start and end lines coincide. Guards the `end_line.max(start_line)`
        // floor and the end-byte-to-line mapping on the smallest construct.
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let e = extent_of(
            "fn tiny() {}\n",
            &language,
            tree_sitter_rust::TAGS_QUERY,
            "tiny",
        );
        assert_eq!((e.start_line, e.end_line), (1, 1));
    }
}
