//! The grammar registry (architecture 5.5.3): the `extension -> (grammar, tag query)` table
//! that decides WHICH grammar a file is parsed under. It supplies the injected pair unit 1's
//! `extract` consumes; it never parses anything itself. Adding a language is adding one match
//! arm here, and a language whose query cannot tag a definition (its grammar gives it no name
//! node, or the upstream query already holds that node as a reference) also adds a read-off in
//! `extract`. Confined to the `symbols` feature because it names `tree_sitter::Language`;
//! the parser-free model stays in the light lane.

use crate::grounder::symbols::model::Lang;
use std::sync::OnceLock;

/// The Rust `tags` query. The upstream `tree-sitter-rust` `tags.scm` records a call only when the
/// callee is a bare identifier or a method (`f()`, `x.f()`), so every path-qualified or turbofish
/// call - `Store::open()`, `crate::sqlite::open_connection()`, `make::<T>()` - left no reference
/// and its caller no call edge. It also tags an impl block only as a reference to its trait (or,
/// for an inherent impl, its type), and only when that name is a bare identifier, and it tags no
/// `const`, `static`, enum variant, field, trait required fn, associated type or `extern crate`.
/// This is the upstream query's call references verbatim, plus the path-qualified and turbofish
/// call shapes, minus the impl-header references, with a definition pattern for every Rust
/// definition type the grammar names (spec 116): impl blocks and tuple fields, which the grammar
/// gives no name node, are read off the parsed tree by `extract` instead. A trait is tagged a
/// trait and a `macro_rules!` a macro; a trait required fn or an extern block's fn is tagged a
/// function, which `extract` lowers to a declaration; an `extern crate` is a module named by its
/// alias when it has one, else by its name.
const RUST_TAGS: &str = r#"
(struct_item name: (type_identifier) @name) @definition.class
(enum_item name: (type_identifier) @name) @definition.class
(union_item name: (type_identifier) @name) @definition.class
(type_item name: (type_identifier) @name) @definition.class
(associated_type name: (type_identifier) @name) @definition.class
(declaration_list (function_item name: (identifier) @name) @definition.method)
(function_item name: (identifier) @name) @definition.function
(function_signature_item name: (identifier) @name) @definition.function
(trait_item name: (type_identifier) @name) @definition.trait
(mod_item name: (identifier) @name) @definition.module
(extern_crate_declaration name: (identifier) @name !alias) @definition.module
(extern_crate_declaration alias: (identifier) @name) @definition.module
(macro_definition name: (identifier) @name) @definition.macro
(const_item name: (identifier) @name) @definition.constant
(static_item name: (identifier) @name) @definition.static
(enum_variant name: (identifier) @name) @definition.variant
(field_declaration name: (field_identifier) @name) @definition.field

(call_expression function: (identifier) @name) @reference.call
(call_expression function: (field_expression field: (field_identifier) @name)) @reference.call
(call_expression function: (scoped_identifier name: (identifier) @name)) @reference.call
(call_expression
  function: (generic_function function: (identifier) @name)) @reference.call
(call_expression
  function: (generic_function function: (field_expression field: (field_identifier) @name))) @reference.call
(call_expression
  function: (generic_function function: (scoped_identifier name: (identifier) @name))) @reference.call
(macro_invocation macro: (identifier) @name) @reference.call
"#;

/// The C# `tags` query. The upstream `tree-sitter-c-sharp` crate ships a `tags.scm` whose last
/// pattern captures a bare `@module`, which `tree-sitter-tags` rejects (it accepts only
/// `@definition.*` / `@reference.*` / `@name` / `@doc` / `@local.*`), so the shipped query fails
/// to compile. That pattern is also redundant: the line above it already tags the same
/// `namespace_declaration` as `@definition.module`. This is the shipped query verbatim minus
/// that one invalid, redundant line - authored here per the spec's "author or verify each
/// grammar's tag query" mandate - with a definition pattern for every C# definition type the
/// grammar names (spec 116). A struct, a record, an enum and a delegate are tagged as a class
/// is; an enum member a variant; a destructor a method, which `extract` names `~` and its
/// identifier; a local function a function; a constructor a constructor; a property and an event
/// with accessors a property. A method, destructor, local function or constructor with no body
/// `extract` lowers to a declaration. A namespace, block or file-scoped, is a module named by
/// its whole name node, so `namespace A.B.C` is `A.B.C`. A `variable_declarator` is a field
/// under a `field_declaration` or an `event_field_declaration` only, so a method-local variable
/// is none, and one declaration naming several is one definition per name; its type sits
/// outside the declarator, so the upstream reference to it stays its container's. Under a
/// `field_declaration` holding the `const` modifier the declarator is a constant: that pattern
/// stands BEFORE the field pattern because both match a `const` field's name node, and one name
/// node carries one tag, the earlier pattern's. An operator, a conversion operator and an
/// indexer have no name node, so `extract` reads them off the parsed tree.
const CSHARP_TAGS: &str = r#"
(class_declaration name: (identifier) @name) @definition.class

(struct_declaration name: (identifier) @name) @definition.class

(record_declaration name: (identifier) @name) @definition.class

(enum_declaration name: (identifier) @name) @definition.class

(delegate_declaration name: (identifier) @name) @definition.class

(enum_member_declaration name: (identifier) @name) @definition.variant

(destructor_declaration name: (identifier) @name) @definition.method

(local_function_statement name: (identifier) @name) @definition.function

(constructor_declaration name: (identifier) @name) @definition.constructor

(property_declaration name: (identifier) @name) @definition.property

(event_declaration name: (identifier) @name) @definition.property

(field_declaration
  (modifier "const")
  (variable_declaration (variable_declarator name: (identifier) @name) @definition.constant))

(field_declaration
  (variable_declaration (variable_declarator name: (identifier) @name) @definition.field))

(event_field_declaration
  (variable_declaration (variable_declarator name: (identifier) @name) @definition.field))

(class_declaration (base_list (_) @name)) @reference.class

(interface_declaration name: (identifier) @name) @definition.interface

(interface_declaration (base_list (_) @name)) @reference.interface

(method_declaration name: (identifier) @name) @definition.method

(object_creation_expression type: (identifier) @name) @reference.class

(type_parameter_constraints_clause (identifier) @name) @reference.class

(type_parameter_constraint (type type: (identifier) @name)) @reference.class

(variable_declaration type: (identifier) @name) @reference.class

(invocation_expression function: (member_access_expression name: (identifier) @name)) @reference.send

(namespace_declaration name: (_) @name) @definition.module

(file_scoped_namespace_declaration name: (_) @name) @definition.module
"#;

/// Additional JS/TS tags-query patterns (spec 92 criterion 2, THE WHOLE PRODUCT IS COVERED): the
/// upstream `tags.scm` tags a `const`/`let`/`var` declarator ONLY when its value is an
/// arrow/function expression (or, for an ES-module-style export, an `export X = <literal>`
/// assignment-form export this project's plain CommonJS-style scripts never use), so a plain
/// top-level constant - a ternary, a boolean coercion, a string, a call result - never became a
/// graph entity at all. `docs/audit/2026-09-graph-vs-grep.md` questions 5 and 8 are exactly this
/// gap, measured against `workflows/rigger.js`'s own `OUTER_WALL_CLOCK_SEC` (a ternary) and
/// `FRESH` (`!!A.fresh`, a unary coercion) constants.
///
/// The value's node type is enumerated EXPLICITLY (never a bare wildcard `(_)`), so this pattern
/// can never also match an `arrow_function` / `function_expression` / `class` /
/// `generator_function` value - those stay the upstream function/class patterns' own captures,
/// never double-tagged here as a SECOND, conflicting definition of the same declarator.
///
/// Appended to BOTH the plain JavaScript query ([`javascript_tags_query`]) and the composed
/// TypeScript one ([`typescript_tags_query`]) - the TypeScript grammar parses this same
/// declarator shape - so the two grammars never diverge on what counts as a constant: one query
/// fragment, not two hand-copies that could drift apart.
const JS_CONSTANT_TAGS: &str = r#"
(
  (comment)* @doc
  .
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name
      value: [
        (number) (string) (true) (false) (null) (undefined) (regex)
        (identifier) (member_expression) (subscript_expression)
        (call_expression) (new_expression) (binary_expression) (unary_expression)
        (ternary_expression) (template_string) (object) (array)
        (parenthesized_expression) (sequence_expression) (await_expression)
      ]) @definition.constant)
  (#strip! @doc "^[\\s\\*/]+|^[\\s\\*/]$")
  (#select-adjacent! @doc @definition.constant)
)

(
  (comment)* @doc
  .
  (variable_declaration
    (variable_declarator
      name: (identifier) @name
      value: [
        (number) (string) (true) (false) (null) (undefined) (regex)
        (identifier) (member_expression) (subscript_expression)
        (call_expression) (new_expression) (binary_expression) (unary_expression)
        (ternary_expression) (template_string) (object) (array)
        (parenthesized_expression) (sequence_expression) (await_expression)
      ]) @definition.constant)
  (#strip! @doc "^[\\s\\*/]+|^[\\s\\*/]$")
  (#select-adjacent! @doc @definition.constant)
)
"#;

/// The JavaScript `tags` query, composed ONCE: the upstream query plus [`JS_CONSTANT_TAGS`].
/// Returned as `&'static str` via a process-lifetime `OnceLock`, mirroring
/// [`typescript_tags_query`] below.
fn javascript_tags_query() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    composed_once(&Q, &[tree_sitter_javascript::TAGS_QUERY, JS_CONSTANT_TAGS])
}

/// The TypeScript `tags` query, composed ONCE. The upstream `tree-sitter-typescript` crate
/// ships a `tags.scm` that tags only TypeScript-specific SIGNATURES (interfaces, ambient
/// function/method signatures, abstract classes, modules) - NOT concrete `function`/`class`/
/// `method` declarations, so ordinary TS/TSX source would index nothing. The TypeScript grammar
/// is a superset of JavaScript, so the JavaScript crate's concrete-declaration rules compile and
/// match against it; composing the JavaScript query (concrete declarations + call/reference
/// rules) with the TypeScript query (the TS-specific signatures) yields a query that tags both.
/// Verified to compile against BOTH the TypeScript and the TSX grammars. Returned as
/// `&'static str` via a process-lifetime `OnceLock` so `LanguageEntry` stays borrow-free.
fn typescript_tags_query() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    composed_once(
        &Q,
        &[
            tree_sitter_javascript::TAGS_QUERY,
            tree_sitter_typescript::TAGS_QUERY,
            JS_CONSTANT_TAGS,
        ],
    )
}

/// `parts` joined one per line into `cell` on first use - a tags query composed ONCE for the
/// process lifetime, so a [`LanguageEntry`] can borrow it as `&'static str`.
fn composed_once(cell: &'static OnceLock<String>, parts: &[&str]) -> &'static str {
    cell.get_or_init(|| parts.join("\n")).as_str()
}

/// Additional Go tags-query patterns (spec 116), appended to the upstream query, which tags a
/// function, a method and a `type_spec` and no other definition: a `const_spec` name is a
/// constant, captured as a direct identifier child: the grammar puts a `const_spec`'s commas
/// inside its `name` field, and a `name:` pattern then matches its first name only, while a
/// `var_spec` and a `field_declaration` field each name alone and match at every one. A
/// `var_spec` name is a variable when its `var_declaration` is a direct child of the source
/// file, grouped or not, so a function-local `var` is none, a named field a field and an
/// interface's `method_elem` a method, which `extract` lowers to a declaration. A spec or field
/// naming several (`const a, b = f(), g()`, a field `x, y Foo`) is one definition per name, all
/// of one range. An embedded field and a `type_alias` have no pattern: the upstream query holds
/// each one's name node as a type reference, and one name node carries one tag, so `extract`
/// reads them off the parsed tree and the reference stays.
const GO_DEFINITION_TAGS: &str = r#"
(const_spec (identifier) @name) @definition.constant
(source_file (var_declaration (var_spec name: (identifier) @name) @definition.variable))
(source_file
  (var_declaration (var_spec_list (var_spec name: (identifier) @name) @definition.variable)))
(field_declaration name: (field_identifier) @name) @definition.field
(method_elem name: (field_identifier) @name) @definition.method
"#;

/// A registered language: the rigger `Lang`, its tree-sitter grammar, and the `tags` query the
/// extractor runs over a file in that language. The three the extractor needs, resolved
/// together so a file maps to exactly one grammar+query pair.
pub struct LanguageEntry {
    pub lang: Lang,
    pub language: tree_sitter::Language,
    pub tags_query: &'static str,
}

/// Resolve the grammar for a bare extension (no leading dot), or `None` when the extension is
/// not one of the shipped languages (the indexer then skips that file). The
/// JavaScript/TypeScript family folds several extensions onto one `Lang`:
/// `.js/.mjs/.cjs/.jsx -> Js`, `.ts/.tsx -> Ts`. `.tsx` resolves to the JSX-aware TSX grammar
/// (`LANGUAGE_TSX`) while `.ts` uses the plain TypeScript grammar - both share the one
/// TypeScript `tags` query, so a `.tsx` file's JSX body parses instead of erroring.
pub fn for_extension(ext: &str) -> Option<LanguageEntry> {
    // The Go `tags` query, composed ONCE: the upstream query plus `GO_DEFINITION_TAGS`.
    static GO_TAGS: OnceLock<String> = OnceLock::new();
    let (lang, language, tags_query): (Lang, tree_sitter::Language, &'static str) = match ext {
        "rs" => (Lang::Rust, tree_sitter_rust::LANGUAGE.into(), RUST_TAGS),
        "cs" => (
            Lang::CSharp,
            tree_sitter_c_sharp::LANGUAGE.into(),
            CSHARP_TAGS,
        ),
        "ts" => (
            Lang::Ts,
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            typescript_tags_query(),
        ),
        "tsx" => (
            Lang::Ts,
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            typescript_tags_query(),
        ),
        "js" | "mjs" | "cjs" | "jsx" => (
            Lang::Js,
            tree_sitter_javascript::LANGUAGE.into(),
            javascript_tags_query(),
        ),
        "go" => (
            Lang::Go,
            tree_sitter_go::LANGUAGE.into(),
            composed_once(&GO_TAGS, &[tree_sitter_go::TAGS_QUERY, GO_DEFINITION_TAGS]),
        ),
        "py" => (
            Lang::Python,
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::TAGS_QUERY,
        ),
        _ => return None,
    };
    Some(LanguageEntry {
        lang,
        language,
        tags_query,
    })
}

/// Resolve a file's grammar, honoring an explicit `--language` override; else auto-detect by
/// extension. An override forces the language for the file regardless of its extension (so a
/// `.txt` can be indexed as Rust); with no override, a file whose extension is unregistered
/// resolves to `None` and is skipped.
pub fn for_path(path: &str, override_lang: Option<Lang>) -> Option<LanguageEntry> {
    let file_ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str());
    if let Some(l) = override_lang {
        // When the file's OWN extension already resolves to the forced language, honor it so a
        // `.tsx` under `--language ts` keeps the JSX-aware grammar rather than collapsing to the
        // plain TypeScript one (both are `Lang::Ts`, but only the `.tsx` entry parses JSX). Only
        // when the extension disagrees (or is unregistered) do we map the forced language to its
        // canonical extension, so `--language rust` on a `.txt` still indexes as Rust. Either
        // branch resolves through the one `for_extension` table.
        if let Some(entry) = file_ext.and_then(for_extension) {
            if entry.lang == l {
                return Some(entry);
            }
        }
        let canonical = match l {
            Lang::Rust => "rs",
            Lang::CSharp => "cs",
            Lang::Ts => "ts",
            Lang::Js => "js",
            Lang::Go => "go",
            Lang::Python => "py",
        };
        return for_extension(canonical);
    }
    for_extension(file_ext?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::csharp_definition_sources::CSHARP_DEFINITIONS_SOURCE;
    use crate::go_definition_sources::GO_DEFINITIONS_SOURCE;
    use crate::grounder::symbols::extract::extract;
    use crate::grounder::symbols::model::{Kind, Lang};

    /// Rust's DEFINITION TYPES (spec 116): each row a node type of the Rust grammar that is a
    /// definition, a kind it lowers to, and the leaf of the one definition of that kind the Rust
    /// definitions fixture `extract.rs`'s tests read holds for it. A `function_item` has a row per
    /// kind it takes; the last two rows are read off the parsed tree, the tuple-field row naming
    /// its list, the grammar giving a tuple field no node of its own.
    pub(crate) const RUST_DEFINITION_TYPES: [(&str, Kind, &str); 19] = [
        ("function_item", Kind::Function, "host"),
        ("function_item", Kind::Method, "width"),
        ("function_item", Kind::Macro, "derive"),
        ("function_signature_item", Kind::Declaration, "paint"),
        ("struct_item", Kind::Type, "Pair"),
        ("union_item", Kind::Type, "Bits"),
        ("enum_item", Kind::Type, "Shape"),
        ("type_item", Kind::Type, "Alias"),
        ("associated_type", Kind::Type, "Canvas"),
        ("trait_item", Kind::Trait, "Draw"),
        ("mod_item", Kind::Module, "nest"),
        ("extern_crate_declaration", Kind::Module, "alloc"),
        ("const_item", Kind::Constant, "LIMIT"),
        ("static_item", Kind::Static, "TABLE"),
        ("enum_variant", Kind::Variant, "Unit"),
        ("field_declaration", Kind::Field, "side"),
        ("macro_definition", Kind::Macro, "shout"),
        ("impl_item", Kind::Impl, "impl Pair"),
        ("ordered_field_declaration_list", Kind::Field, "1"),
    ];

    /// Go's DEFINITION TYPES (spec 116): each row a node type of the Go grammar that is a
    /// definition, a kind it lowers to, and the leaf of the one definition of that kind the Go
    /// definitions fixture holds for it. A `function_declaration` and a `method_declaration`
    /// have a row with a body and one without; a `var_spec` is a definition at package level
    /// only; the two `field_declaration` rows are a named field and an embedded one, which is
    /// read off the parsed tree as the `type_alias` row is.
    pub(crate) const GO_DEFINITION_TYPES: [(&str, Kind, &str); 11] = [
        ("function_declaration", Kind::Function, "host"),
        ("function_declaration", Kind::Declaration, "ext"),
        ("method_declaration", Kind::Method, "area"),
        ("method_declaration", Kind::Declaration, "bare"),
        ("method_elem", Kind::Declaration, "Draw"),
        ("type_spec", Kind::Type, "Shape"),
        ("const_spec", Kind::Constant, "Limit"),
        ("var_spec", Kind::Variable, "single"),
        ("field_declaration", Kind::Field, "side"),
        ("field_declaration", Kind::Field, "Ptr"),
        ("type_alias", Kind::Type, "Alias"),
    ];

    /// C#'s DEFINITION TYPES (spec 116): each row a node type of the C# grammar that is a
    /// definition, a kind it lowers to, and the leaf of the one definition of that kind the C#
    /// definitions fixture holds for it. Each of the six types a body is optional on has a row
    /// with a body and one without; a `variable_declarator` is a definition under a field or an
    /// event field declaration only, a constant where its field declaration is `const`; the
    /// last five rows are read off the parsed tree.
    pub(crate) const CSHARP_DEFINITION_TYPES: [(&str, Kind, &str); 27] = [
        ("class_declaration", Kind::Type, "Foo"),
        ("struct_declaration", Kind::Type, "Cell"),
        ("record_declaration", Kind::Type, "Rec"),
        ("interface_declaration", Kind::Type, "IDraw"),
        ("enum_declaration", Kind::Type, "Hue"),
        ("delegate_declaration", Kind::Type, "Fold"),
        ("enum_member_declaration", Kind::Variant, "Red"),
        ("method_declaration", Kind::Method, "Run"),
        ("method_declaration", Kind::Declaration, "Paint"),
        ("destructor_declaration", Kind::Method, "~Foo"),
        ("destructor_declaration", Kind::Declaration, "~Ext"),
        ("local_function_statement", Kind::Function, "Inner"),
        ("local_function_statement", Kind::Declaration, "Outer"),
        ("constructor_declaration", Kind::Constructor, "Foo"),
        ("constructor_declaration", Kind::Declaration, "Ext"),
        ("property_declaration", Kind::Property, "Size"),
        ("event_declaration", Kind::Property, "Moved"),
        ("namespace_declaration", Kind::Module, "A.B.C"),
        ("file_scoped_namespace_declaration", Kind::Module, "S.T"),
        ("variable_declarator", Kind::Field, "owner"),
        ("variable_declarator", Kind::Constant, "Max"),
        ("variable_declarator", Kind::Field, "Changed"),
        ("operator_declaration", Kind::Method, "+"),
        ("operator_declaration", Kind::Declaration, "-"),
        (
            "conversion_operator_declaration",
            Kind::Method,
            "implicit operator int",
        ),
        (
            "conversion_operator_declaration",
            Kind::Declaration,
            "explicit operator Ext",
        ),
        ("indexer_declaration", Kind::Method, "this"),
    ];

    #[test]
    fn auto_detects_all_five_languages_by_extension() {
        assert_eq!(for_extension("rs").unwrap().lang, Lang::Rust);
        assert_eq!(for_extension("cs").unwrap().lang, Lang::CSharp);
        assert_eq!(for_extension("ts").unwrap().lang, Lang::Ts);
        assert_eq!(for_extension("tsx").unwrap().lang, Lang::Ts);
        assert_eq!(for_extension("js").unwrap().lang, Lang::Js);
        assert_eq!(for_extension("mjs").unwrap().lang, Lang::Js);
        assert_eq!(for_extension("cjs").unwrap().lang, Lang::Js);
        assert_eq!(for_extension("jsx").unwrap().lang, Lang::Js);
        assert_eq!(for_extension("go").unwrap().lang, Lang::Go);
        assert_eq!(for_extension("py").unwrap().lang, Lang::Python);
        // An unregistered extension resolves to nothing (the file is skipped by the indexer).
        assert!(for_extension("txt").is_none());
    }

    #[test]
    fn language_override_wins_over_extension() {
        // A `.txt` with an explicit override still resolves (the `--language` override).
        assert_eq!(
            for_path("notes.txt", Some(Lang::Rust)).unwrap().lang,
            Lang::Rust
        );
        // No override + unknown extension: none.
        assert!(for_path("notes.txt", None).is_none());
        // No override + known extension auto-detects.
        assert_eq!(for_path("src/main.rs", None).unwrap().lang, Lang::Rust);
        // The override maps to a working, extractable grammar for every language, not just a
        // `Lang` tag: overriding a `.txt` to Go resolves an entry that extracts Go.
        let go = for_path("notes.txt", Some(Lang::Go)).unwrap();
        assert_eq!(go.lang, Lang::Go);
        assert!(!go.tags_query.is_empty());
    }

    #[test]
    fn tsx_resolves_to_the_jsx_aware_grammar_ts_to_plain() {
        // Both are `Lang::Ts`, but a `.tsx` file must parse under the TSX (JSX-aware) grammar,
        // not the plain TypeScript one, or JSX bodies fail to parse. The registry owns WHICH
        // grammar a file uses (spec 15 unit 2), so this is exercised end to end below.
        // A `.tsx` component with a JSX body still yields its function definition (the plain TS
        // grammar would error on the JSX).
        let src = "function Panel() { return <div>{title}</div>; }\n";
        let fs = crate::symbol_index::extracted("tsx", src);
        assert!(
            fs.defs.iter().any(|d| d.name == "Panel"),
            "tsx grammar should extract the Panel component def, got {:?}",
            fs.defs
        );
    }

    #[test]
    fn language_override_keeps_the_tsx_jsx_grammar_for_a_tsx_file() {
        // `--language ts` on a `.tsx` file must still parse the JSX body: the override forces the
        // LANGUAGE, but the file's own extension refines WHICH grammar (JSX-aware vs plain). The
        // plain TypeScript grammar errors on JSX, so a mis-resolved override would drop the def.
        let entry = for_path("Panel.tsx", Some(Lang::Ts)).unwrap();
        assert_eq!(entry.lang, Lang::Ts);
        let src = "function Panel() { return <div>{title}</div>; }\n";
        let fs = extract(src, entry.lang, &entry.language, entry.tags_query).unwrap();
        assert!(
            fs.defs.iter().any(|d| d.name == "Panel"),
            "override --language ts on a .tsx file should keep the JSX-aware grammar, got {:?}",
            fs.defs
        );
        // The forcing behavior is preserved where the extension DISAGREES: `--language ts` on a
        // `.rs` file still forces TypeScript (the plain grammar, since `.rs` is not a TS file).
        assert_eq!(for_path("main.rs", Some(Lang::Ts)).unwrap().lang, Lang::Ts);
    }

    #[test]
    fn every_registered_language_resolves_and_extracts_a_definition() {
        // Criterion 2: the registry resolves AND extracts symbols for all five shipped
        // languages. One fixture per language, each a canonical top-level definition, fed
        // through the SAME injected-grammar extraction path (unit 1) the registry supplies.
        let cases: &[(&str, &str, &str)] = &[
            ("rs", "fn parse() {}\n", "parse"),
            ("cs", "class Widget { public void Draw() {} }\n", "Widget"),
            ("js", "function greet() { return 1; }\n", "greet"),
            ("ts", "function greet(): number { return 1; }\n", "greet"),
            ("go", "package main\nfunc Handle() {}\n", "Handle"),
            ("py", "def compute():\n    return 1\n", "compute"),
        ];
        for (ext, src, expected) in cases {
            let entry = for_extension(ext)
                .unwrap_or_else(|| panic!("extension {ext} should be registered"));
            let fs = extract(src, entry.lang, &entry.language, entry.tags_query)
                .unwrap_or_else(|e| panic!("extraction failed for .{ext}: {e}"));
            assert!(
                fs.defs.iter().any(|d| d.name == *expected),
                ".{ext} should extract a `{expected}` definition, got defs {:?}",
                fs.defs
            );
        }
    }

    #[test]
    fn js_plain_const_declarations_extract_as_constant_definitions() {
        // Spec 92 criterion 2 (THE WHOLE PRODUCT IS COVERED): the upstream
        // tree-sitter-javascript tags.scm tags a `const`/`let` ONLY when its value is an
        // arrow/function expression, or an ES `export X = <literal>` assignment-form export -
        // a syntax this project's plain CommonJS-style `workflows/rigger.js` never uses. A
        // plain top-level constant therefore never became a graph entity at all - exactly
        // the gap docs/audit/2026-09-graph-vs-grep.md questions 5 and 8 measured against that
        // file's own `OUTER_WALL_CLOCK_SEC` (a ternary) and `FRESH` (a boolean coercion)
        // constants. Both must now extract as `Kind::Constant` definitions.
        let src = "const OUTER_WALL_CLOCK_SEC = Number(x) > 0 ? Number(x) : 900\nconst FRESH = !!a.fresh\n";
        let fs = crate::symbol_index::extracted("js", src);
        assert!(
            fs.defs
                .iter()
                .any(|d| d.name == "OUTER_WALL_CLOCK_SEC" && d.kind == Kind::Constant),
            "a ternary-valued top-level const must extract as a Kind::Constant definition, \
             got {:?}",
            fs.defs
        );
        assert!(
            fs.defs
                .iter()
                .any(|d| d.name == "FRESH" && d.kind == Kind::Constant),
            "a boolean-coercion-valued top-level const must extract as a Kind::Constant \
             definition, got {:?}",
            fs.defs
        );
        // A function-valued const (already covered by the upstream query) must NOT also
        // double-tag as a constant - the new pattern is additive only for non-function values.
        let fn_src = "const greet = () => 1\n";
        let fn_fs = crate::symbol_index::extracted("js", fn_src);
        assert!(
            fn_fs
                .defs
                .iter()
                .any(|d| d.name == "greet" && d.kind == Kind::Function),
            "an arrow-valued const still extracts as a function, got {:?}",
            fn_fs.defs
        );
        assert_eq!(
            fn_fs.defs.iter().filter(|d| d.name == "greet").count(),
            1,
            "an arrow-valued const must extract exactly ONCE (as a function), never a second \
             time as a constant, got {:?}",
            fn_fs.defs
        );
    }

    /// A REFERENCE'S CALLER IS ITS FUNCTION (spec 116 criterion 2), under the composed JS query:
    /// a call in a function-local `const x = f()` is its function's, whether that function is a
    /// declaration or an arrow-valued const, each local const still a constant definition; a
    /// top-level const no function encloses keeps its own call, and a top-level call has the
    /// file, no caller.
    #[test]
    fn a_call_in_a_function_local_js_const_is_attributed_to_its_function() {
        let fs = crate::symbol_index::extracted("js", crate::caller_sources::JS_CALLER_SOURCE);
        assert_eq!(
            crate::symbol_index::defs_by_line(&fs),
            vec![
                (1, "outer", Kind::Function),
                (2, "x", Kind::Constant),
                (4, "y", Kind::Constant),
                (5, "h", Kind::Function),
                (6, "z", Kind::Constant),
            ],
            "each function and each const, local or not, is a definition"
        );
        assert_eq!(
            crate::symbol_index::refs_with_callers(&fs),
            vec![
                (2, "f", Some("outer")),
                (4, "g", Some("y")),
                (6, "k", Some("h")),
                (8, "top", None),
            ],
            "a function-local const's call is its function's, a top-level const's its own, a \
             top-level call the file's"
        );
    }

    /// A persisted index stamped `ts-tags-v2`, the generation before the caller rule changed
    /// what `SymRef.enclosing` holds, is not current and loads as absent; one stamped
    /// `ts-tags-v3` is current and loads.
    #[test]
    fn a_persisted_ts_tags_v2_index_is_not_current() {
        use crate::grounder::symbols::model::SymbolIndex;
        use crate::grounder::symbols::store;
        let persisted = |grammar: &str| -> (bool, bool) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_str().unwrap();
            crate::symbol_index::persist_stamped(root, grammar, r#"{"files":{}}"#);
            let index: SymbolIndex =
                serde_json::from_slice(&std::fs::read(store::index_path(root)).unwrap()).unwrap();
            (index.is_current(), store::load(root).is_some())
        };
        assert_eq!(
            persisted("ts-tags-v2"),
            (false, false),
            "a ts-tags-v2 index is not current and loads as absent"
        );
        assert_eq!(
            persisted("ts-tags-v3"),
            (true, true),
            "a ts-tags-v3 index is current and loads"
        );
    }

    /// THE NAME FILTER DROPS A NAMELESS DEFINITION (spec 116 criterion 3), under the registered Go
    /// query: a function, a method and a type named `_`, Go's blank identifier, make no
    /// definition, while the named type and function beside them keep theirs; the call inside
    /// `func _()` has the file, no caller, as have the receiver type and the call of
    /// `func (T) _()` and the underlying type of `type _ int`, a named function's call still
    /// being its own.
    #[test]
    fn a_go_definition_named_with_the_blank_identifier_makes_no_def() {
        let fs = crate::symbol_index::extracted("go", crate::name_filter_sources::GO_BLANK_SOURCE);
        assert_eq!(
            crate::symbol_index::defs_by_line(&fs),
            vec![(2, "T", Kind::Type), (6, "named", Kind::Function)],
            "`func _()`, `func (T) _()` and `type _ int` make no definition"
        );
        assert_eq!(
            crate::symbol_index::refs_with_callers(&fs),
            vec![
                (3, "probe", None),
                (4, "T", None),
                (4, "reach", None),
                (5, "int", None),
                (6, "other", Some("named")),
            ],
            "every reference inside a dropped definition takes the file as its caller"
        );
    }

    /// The name filter keeps `_` where it is an ordinary name: a JS `const _ = require(..)` and a
    /// Python `_ = gettext.gettext` each keep their definition under the composed JS query and
    /// the upstream Python one; neither source holds a reference, the upstream JS query tagging
    /// no `require` call and a Python attribute read being no call.
    #[test]
    fn a_js_and_a_python_definition_named_underscore_keeps_its_def() {
        let js =
            crate::symbol_index::extracted("js", crate::name_filter_sources::JS_UNDERSCORE_SOURCE);
        assert_eq!(
            crate::symbol_index::defs_by_line(&js),
            vec![(1, "_", Kind::Constant)],
            "a JS const named `_` is a definition"
        );
        assert_eq!(
            crate::symbol_index::refs_with_callers(&js),
            vec![],
            "the upstream JS query tags no `require` call"
        );
        let py =
            crate::symbol_index::extracted("py", crate::name_filter_sources::PY_UNDERSCORE_SOURCE);
        assert_eq!(
            crate::symbol_index::defs_by_line(&py),
            vec![(2, "_", Kind::Constant)],
            "a Python module-level `_` is a definition"
        );
        assert_eq!(
            crate::symbol_index::refs_with_callers(&py),
            vec![],
            "a Python attribute read is no call"
        );
    }

    crate::test_cases! {
        /// GO DEFINITIONS ARE NODES (spec 116 criterion 7): under the registered Go query the
        /// fixture yields every definition it holds at its own kind and leaf: each name of a const
        /// spec a constant, a function-local const included; a package-level `var`, single, grouped
        /// or naming two, a variable, and the function-local `var local` and grouped `var deep`
        /// none; a named field a field, each name of a field declaration naming two included; an
        /// embedded field a field at its type's base identifier, pointer, package qualifier and
        /// type arguments stripped; a type alias a type, the one named `_` none; an interface's
        /// method and a function and a method with no body each a declaration. Each row of Go's
        /// DEFINITION TYPES names exactly one of them.
        ///
        /// The callers: both calls of `const a, b = f(), g()` are `a`'s and both of `var m, n =
        /// h(), k()` are `m`'s, the least name among the definitions of one range; an embedded
        /// field keeps the reference to its type, the field its caller, a generic one its type
        /// argument's too; a named field's type reference is the field's and that of a field
        /// declaration naming two its least name's; an alias's name and its target are the alias's;
        /// a call in a function-local const is its function's; an interface method's parameter type
        /// is the interface's, a declaration being no caller, so that of a function or a method
        /// with no body is the file's; and both references of the alias named `_` are the file's.
        every_go_definition_type_yields_its_definition_at_its_kind:
            crate::symbol_index::assert_definitions_fixture(
                "go",
                GO_DEFINITIONS_SOURCE,
                &GO_DEFINITION_TYPES,
                &[
                    (2, "Limit", Kind::Constant),
                    (3, "a", Kind::Constant),
                    (3, "b", Kind::Constant),
                    (4, "single", Kind::Variable),
                    (5, "grouped", Kind::Variable),
                    (6, "m", Kind::Variable),
                    (6, "n", Kind::Variable),
                    (7, "Base", Kind::Type),
                    (8, "Base", Kind::Field),
                    (8, "Gen", Kind::Field),
                    (8, "Ptr", Kind::Field),
                    (8, "Qual", Kind::Field),
                    (8, "Shape", Kind::Type),
                    (8, "side", Kind::Field),
                    (8, "x", Kind::Field),
                    (8, "y", Kind::Field),
                    (9, "Alias", Kind::Type),
                    (10, "Draw", Kind::Declaration),
                    (10, "Drawer", Kind::Type),
                    (11, "host", Kind::Function),
                    (11, "inner", Kind::Constant),
                    (12, "area", Kind::Method),
                    (13, "ext", Kind::Declaration),
                    (14, "bare", Kind::Declaration),
                ],
                &[
                    (3, "f", Some("a")),
                    (3, "g", Some("a")),
                    (6, "h", Some("m")),
                    (6, "k", Some("m")),
                    (8, "Base", Some("Base")),
                    (8, "Coord", Some("x")),
                    (8, "Gen", Some("Gen")),
                    (8, "Ptr", Some("Ptr")),
                    (8, "Qual", Some("Qual")),
                    (8, "Side", Some("side")),
                    (8, "int", Some("Gen")),
                    (9, "Alias", Some("Alias")),
                    (9, "Base", Some("Alias")),
                    (10, "Canvas", Some("Drawer")),
                    (11, "probe", Some("host")),
                    (12, "Shape", Some("area")),
                    (13, "Canvas", None),
                    (14, "Shape", None),
                    (15, "Base", None),
                    (15, "_", None),
                ],
            );
        /// C# DEFINITIONS ARE NODES (spec 116 criterion 8): under `CSHARP_TAGS` the fixture yields
        /// every definition it holds at its own kind and leaf: a class, a struct, a record, an
        /// interface, an enum and a delegate each a type; an enum member a variant; a property and
        /// an event with accessors each a property; a field and each name of a field declaration
        /// naming two a field, an event field too, and a `const` field a constant, whatever other
        /// modifier it carries; a constructor a constructor at its class's leaf; a local function a
        /// function; and the method-local `var local` and `const deep` none. `namespace A.B.C` and
        /// the file-scoped `namespace S.T` are modules at their whole dotted text; the destructor
        /// is a method at `~Foo`, the operator at `+`, the conversion operator at `implicit
        /// operator int`, its header's whitespace collapsed, and the indexer at `this`. A method, a
        /// destructor, a local function, a constructor, an operator and a conversion operator
        /// holding no body are each a declaration. Each row of C#'s DEFINITION TYPES names exactly
        /// one of them.
        ///
        /// The callers: a field's type reference is its struct's, the declarator being the field,
        /// and its initializer's is the field's; the reference in an event's accessor is the
        /// event's and in a property's value the property's; that in a constructor body is named by
        /// the leaf its class shares; those in a destructor, an operator, a conversion operator and
        /// an indexer are each named by that member's leaf; and that in a local function is the
        /// local function's, the method keeping its own.
        every_csharp_definition_type_yields_its_definition_at_its_kind:
            crate::symbol_index::assert_definitions_fixture(
                "cs",
                CSHARP_DEFINITIONS_SOURCE,
                &CSHARP_DEFINITION_TYPES,
                &[
                    (1, "S.T", Kind::Module),
                    (2, "A.B.C", Kind::Module),
                    (2, "Go", Kind::Method),
                    (2, "Probe", Kind::Type),
                    (3, "Fold", Kind::Type),
                    (4, "Blue", Kind::Variant),
                    (4, "Hue", Kind::Type),
                    (4, "Red", Kind::Variant),
                    (5, "Rec", Kind::Type),
                    (6, "IDraw", Kind::Type),
                    (6, "Paint", Kind::Declaration),
                    (7, "Cell", Kind::Type),
                    (7, "Max", Kind::Constant),
                    (7, "Min", Kind::Constant),
                    (7, "Seed", Kind::Field),
                    (7, "hi", Kind::Field),
                    (7, "lo", Kind::Field),
                    (7, "made", Kind::Field),
                    (7, "owner", Kind::Field),
                    (8, "Foo", Kind::Type),
                    (9, "Changed", Kind::Field),
                    (10, "Moved", Kind::Property),
                    (11, "Size", Kind::Property),
                    (12, "Foo", Kind::Constructor),
                    (13, "~Foo", Kind::Method),
                    (14, "+", Kind::Method),
                    (15, "implicit operator int", Kind::Method),
                    (16, "this", Kind::Method),
                    (17, "Inner", Kind::Function),
                    (17, "Outer", Kind::Declaration),
                    (17, "Run", Kind::Method),
                    (19, "-", Kind::Declaration),
                    (19, "Bare", Kind::Declaration),
                    (19, "Ext", Kind::Type),
                    (19, "Ext", Kind::Declaration),
                    (19, "explicit operator Ext", Kind::Declaration),
                    (19, "~Ext", Kind::Declaration),
                ],
                &[
                    (7, "Probe", Some("Cell")),
                    (7, "Probe", Some("Cell")),
                    (7, "Probe", Some("made")),
                    (10, "Probe", Some("Moved")),
                    (11, "Probe", Some("Size")),
                    (12, "Probe", Some("Foo")),
                    (13, "Probe", Some("~Foo")),
                    (14, "Go", Some("+")),
                    (14, "Probe", Some("+")),
                    (15, "Probe", Some("implicit operator int")),
                    (16, "Probe", Some("this")),
                    (17, "Probe", Some("Inner")),
                    (17, "Probe", Some("Run")),
                ],
            );
    }
}
