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
/// `namespace_declaration` as `@definition.module`. This is the shipped query without that one
/// invalid, redundant line - authored here per the spec's "author or verify each grammar's tag
/// query" mandate - its namespace pattern taking any name node where the shipped one takes an
/// identifier, plus a definition pattern for every other C# definition type the grammar names
/// (spec 116). A struct, a record, an enum and a delegate are tagged as a class
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

/// Additional JS tags-query patterns (spec 116), appended to the upstream query in all three of
/// the JS, TS and TSX queries, every grammar of the family holding each node type they name.
/// The upstream query tags a method named by a `property_identifier` only, and never one named
/// `constructor`: a private (`#m`), string (`"a-b"`) or numeric (`1`) method name is a method
/// too, whose leaf `extract` reads, and a method named `constructor` is a constructor; a
/// computed name (`[k]`) has no pattern, nothing naming it statically. A declarator takes its
/// value's kind: the upstream query tags an arrow function or function expression value a
/// function, and a generator function value is one too; a class value, anonymous or named, is a
/// type named by the declarator (a named class keeping its own definition); and a declarator
/// with no value is a constant, as is every other value ([`JS_CONSTANT_VALUES`]). A declarator
/// named by a pattern has no pattern here: `extract` walks the identifiers it binds.
const JS_DEFINITION_TAGS: &str = r#"
(method_definition
  name: [(private_property_identifier) (string) (number)] @name) @definition.method
((method_definition name: (property_identifier) @name) @definition.constructor
  (#eq? @name "constructor"))
(variable_declarator name: (identifier) @name value: (generator_function)) @definition.function
(variable_declarator name: (identifier) @name value: (class)) @definition.class
(variable_declarator name: (identifier) @name !value) @definition.constant
"#;

/// Additional TS tags-query patterns (spec 116), appended to the TS and TSX queries: a string or
/// numeric method signature or abstract method name, which upstream leaves untagged, as a method
/// (`extract` lowers every signature to a declaration); an enum and a type alias as a type; a
/// module named by a string (`declare module "x"`) or a dotted name, and a `namespace` of any
/// name, as a module; an enum member with a value as a variant (one without is read off the
/// enum's body by `extract`, the grammar holding it as the body's own `name`); and an
/// interface's property signature and a class's field as a field, which `extract` lowers to a
/// method when its value is a function. A private name (`#p`) is in the alternation of a class
/// field alone, the one member of these TypeScript lets a private name declare.
const TS_DEFINITION_TAGS: &str = r#"
(method_signature name: [(string) (number)] @name) @definition.method
(abstract_method_signature name: [(string) (number)] @name) @definition.method
(enum_declaration name: (identifier) @name) @definition.class
(type_alias_declaration name: (type_identifier) @name) @definition.type
(module name: [(string) (nested_identifier)] @name) @definition.module
(internal_module name: (_) @name) @definition.module
(enum_assignment name: [(property_identifier) (string) (number)] @name) @definition.variant
(property_signature name: [(property_identifier) (string) (number)] @name) @definition.field
(public_field_definition
  name: [(property_identifier) (private_property_identifier) (string) (number)] @name)
  @definition.field
"#;

/// The opening of the JS constant pattern (spec 92 criterion 2, spec 116): a declarator named by
/// an identifier whose value is one of an enumerated list of node types is a constant. The list
/// follows in the parts each grammar's query composes after this one, [`JS_CONSTANT_VALUES`]
/// and that grammar's own types, and [`CONSTANT_TAGS_CLOSE`] ends it. The value types are
/// enumerated EXPLICITLY (never a bare wildcard `(_)`): every `expression` subtype of the
/// grammar but a function expression, an arrow function, a generator function and a class,
/// which the function and class patterns tag, so a declarator never carries a second,
/// conflicting kind.
const CONSTANT_TAGS_OPEN: &str = "(variable_declarator name: (identifier) @name value: [";

/// The `expression` subtypes, flattened through `primary_expression`, that the JS, TS and TSX
/// grammars all hold and that make a declarator a constant.
const JS_CONSTANT_VALUES: &str = "(array) (assignment_expression) \
(augmented_assignment_expression) (await_expression) (binary_expression) (call_expression) \
(false) (identifier) (member_expression) (meta_property) (new_expression) (null) (number) \
(object) (parenthesized_expression) (regex) (string) (subscript_expression) (super) \
(template_string) (ternary_expression) (this) (true) (unary_expression) (undefined) \
(update_expression) (yield_expression)";

/// The constant value types the JS and TSX grammars hold and the TS grammar does not.
const JSX_CONSTANT_VALUES: &str = "(jsx_element) (jsx_self_closing_element)";

/// The constant value types the TS and TSX grammars hold and the JS grammar does not; the TS
/// grammar alone holds a `type_assertion` besides.
const TS_CONSTANT_VALUES: &str = "(as_expression) (satisfies_expression) (non_null_expression) \
(instantiation_expression) (internal_module)";

/// The close of the pattern [`CONSTANT_TAGS_OPEN`] opens.
const CONSTANT_TAGS_CLOSE: &str = "]) @definition.constant";

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

/// The Python `tags` query. The upstream `tree-sitter-python` `tags.scm` tags a module-level
/// assignment whose target is an identifier a constant whatever its value, so a chained
/// assignment (`x = y = f()`) tags its first target alone over the whole chain, every call in it
/// that target's. This is the upstream query with that pattern kept only where the assignment
/// has no value (`w: int`) or a value of one of the types the grammar admits but `assignment`,
/// so a single-target constant stays a tag over the whole assignment, while every target of a
/// chain, the names a pattern binds, a class body's targets and a `type X = ..` alias are read
/// off the parsed tree by `extract`. Every def is tagged a function, which `extract` lowers to a
/// method when its nearest enclosing definition is a class.
const PYTHON_TAGS: &str = r#"
(module (expression_statement (assignment left: (identifier) @name !right) @definition.constant))
(module
  (expression_statement
    (assignment
      left: (identifier) @name
      right: [(augmented_assignment) (expression) (expression_list) (pattern_list) (yield)])
    @definition.constant))

(class_definition
  name: (identifier) @name) @definition.class

(function_definition
  name: (identifier) @name) @definition.function

(call
  function: [
      (identifier) @name
      (attribute
        attribute: (identifier) @name)
  ]) @reference.call
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
/// (`LANGUAGE_TSX`), so a `.tsx` file's JSX body parses instead of erroring, while `.ts` uses
/// the plain TypeScript grammar; each takes its own query, whose constant pattern enumerates
/// its own grammar's value types (TSX reads `<T>x` as JSX, so only TS holds a
/// `type_assertion` and only TSX the JSX elements).
pub fn for_extension(ext: &str) -> Option<LanguageEntry> {
    // The Go `tags` query, composed ONCE: the upstream query plus `GO_DEFINITION_TAGS`.
    static GO_TAGS: OnceLock<String> = OnceLock::new();
    // The JS, TS and TSX `tags` queries, each composed ONCE. The upstream
    // `tree-sitter-typescript` crate ships a `tags.scm` that tags only TypeScript-specific
    // SIGNATURES (interfaces, ambient function/method signatures, abstract classes, modules),
    // NOT concrete `function`/`class`/`method` declarations, so ordinary TS source would index
    // nothing. The TypeScript grammars are supersets of JavaScript, so the JavaScript crate's
    // concrete-declaration rules compile and match against them: the TS and TSX queries compose
    // the upstream JS query (concrete declarations and call/reference rules) with the upstream
    // TS one, `JS_DEFINITION_TAGS` and `TS_DEFINITION_TAGS`, the JS query the upstream JS query
    // with `JS_DEFINITION_TAGS`; each closes with the constant pattern over its own grammar's
    // value types, so none of the three compiles against another's grammar.
    static JS_TAGS: OnceLock<String> = OnceLock::new();
    static TS_TAGS: OnceLock<String> = OnceLock::new();
    static TSX_TAGS: OnceLock<String> = OnceLock::new();
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
            composed_once(
                &TS_TAGS,
                &[
                    tree_sitter_javascript::TAGS_QUERY,
                    tree_sitter_typescript::TAGS_QUERY,
                    JS_DEFINITION_TAGS,
                    TS_DEFINITION_TAGS,
                    CONSTANT_TAGS_OPEN,
                    JS_CONSTANT_VALUES,
                    TS_CONSTANT_VALUES,
                    "(type_assertion)",
                    CONSTANT_TAGS_CLOSE,
                ],
            ),
        ),
        "tsx" => (
            Lang::Ts,
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            composed_once(
                &TSX_TAGS,
                &[
                    tree_sitter_javascript::TAGS_QUERY,
                    tree_sitter_typescript::TAGS_QUERY,
                    JS_DEFINITION_TAGS,
                    TS_DEFINITION_TAGS,
                    CONSTANT_TAGS_OPEN,
                    JS_CONSTANT_VALUES,
                    TS_CONSTANT_VALUES,
                    JSX_CONSTANT_VALUES,
                    CONSTANT_TAGS_CLOSE,
                ],
            ),
        ),
        "js" | "mjs" | "cjs" | "jsx" => (
            Lang::Js,
            tree_sitter_javascript::LANGUAGE.into(),
            composed_once(
                &JS_TAGS,
                &[
                    tree_sitter_javascript::TAGS_QUERY,
                    JS_DEFINITION_TAGS,
                    CONSTANT_TAGS_OPEN,
                    JS_CONSTANT_VALUES,
                    JSX_CONSTANT_VALUES,
                    CONSTANT_TAGS_CLOSE,
                ],
            ),
        ),
        "go" => (
            Lang::Go,
            tree_sitter_go::LANGUAGE.into(),
            composed_once(&GO_TAGS, &[tree_sitter_go::TAGS_QUERY, GO_DEFINITION_TAGS]),
        ),
        "py" => (
            Lang::Python,
            tree_sitter_python::LANGUAGE.into(),
            PYTHON_TAGS,
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
    use crate::js_ts_definition_sources::{
        JS_DEFINITIONS_SOURCE, TSX_DEFINITIONS_SOURCE, TS_DEFINITIONS_SOURCE,
    };
    use crate::python_definition_sources::PYTHON_DEFINITIONS_SOURCE;

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

    /// JS's DEFINITION TYPES (spec 116): each row a node type of the JS grammar that is a
    /// definition, a kind it lowers to, and the leaf of the one definition of that kind the JS
    /// definitions fixture holds for it. A `variable_declarator` takes its value's kind, a
    /// function, a type or a constant, and one named by a pattern makes each identifier it binds
    /// a constant; a `method_definition` named `constructor` is a constructor. The
    /// `field_definition` rows, a field and a function-valued field, and the `for_in_statement`
    /// row are read off the parsed tree; the last three rows are the upstream query's
    /// definitions by a member assignment, an object pair and an exported assignment.
    pub(crate) const JS_DEFINITION_TYPES: [(&str, Kind, &str); 18] = [
        ("function_declaration", Kind::Function, "host"),
        ("generator_function_declaration", Kind::Function, "gen"),
        ("function_expression", Kind::Function, "fexp"),
        ("generator_function", Kind::Function, "gexp"),
        ("method_definition", Kind::Method, "area"),
        ("method_definition", Kind::Constructor, "constructor"),
        ("class_declaration", Kind::Type, "Shape"),
        ("class", Kind::Type, "Baz"),
        ("variable_declarator", Kind::Function, "g"),
        ("variable_declarator", Kind::Type, "Foo"),
        ("variable_declarator", Kind::Constant, "loc"),
        ("variable_declarator", Kind::Constant, "a"),
        ("field_definition", Kind::Field, "field"),
        ("field_definition", Kind::Method, "act"),
        ("for_in_statement", Kind::Constant, "j"),
        ("assignment_expression", Kind::Function, "handler"),
        ("pair", Kind::Function, "p"),
        ("assignment_expression", Kind::Constant, "ex"),
    ];

    /// TS's DEFINITION TYPES (spec 116), the TS and TSX grammars' alike: each row a node type
    /// that is a definition, a kind it lowers to, and the leaf of the one definition of that
    /// kind the TS and the TSX definitions fixtures each hold for it. JS's rows but its
    /// `field_definition` ones, whose class field the TS grammars name a
    /// `public_field_definition`, then the TS types: the three signature types are
    /// declarations, and the last row, an enum member with no value, is read off the parsed
    /// tree, its type column the member's own node, never its `enum_body` container.
    pub(crate) const TS_DEFINITION_TYPES: [(&str, Kind, &str); 30] = [
        ("function_declaration", Kind::Function, "host"),
        ("generator_function_declaration", Kind::Function, "gen"),
        ("function_expression", Kind::Function, "fexp"),
        ("generator_function", Kind::Function, "gexp"),
        ("method_definition", Kind::Method, "area"),
        ("method_definition", Kind::Constructor, "constructor"),
        ("class_declaration", Kind::Type, "Shape"),
        ("class", Kind::Type, "Baz"),
        ("variable_declarator", Kind::Function, "g"),
        ("variable_declarator", Kind::Type, "Foo"),
        ("variable_declarator", Kind::Constant, "loc"),
        ("variable_declarator", Kind::Constant, "a"),
        ("for_in_statement", Kind::Constant, "j"),
        ("assignment_expression", Kind::Function, "handler"),
        ("pair", Kind::Function, "p"),
        ("assignment_expression", Kind::Constant, "ex"),
        ("public_field_definition", Kind::Field, "field"),
        ("public_field_definition", Kind::Method, "act"),
        ("function_signature", Kind::Declaration, "sig"),
        ("method_signature", Kind::Declaration, "m"),
        ("abstract_method_signature", Kind::Declaration, "run"),
        ("abstract_class_declaration", Kind::Type, "Abs"),
        ("interface_declaration", Kind::Type, "Shape2"),
        ("enum_declaration", Kind::Type, "Color"),
        ("type_alias_declaration", Kind::Type, "Alias"),
        ("module", Kind::Module, "x"),
        ("internal_module", Kind::Module, "A.B"),
        ("enum_assignment", Kind::Variant, "Blue"),
        ("property_signature", Kind::Field, "prop"),
        ("property_identifier", Kind::Variant, "Red"),
    ];

    /// Python's DEFINITION TYPES (spec 116): each row a node type of the Python grammar that is a
    /// definition, a kind it lowers to, and the leaf of the one definition of that kind the Python
    /// definitions fixture holds for it. A `function_definition` is a method in a class, else a
    /// function; an `assignment` makes each identifier its targets bind a constant at module
    /// level and a field in a class body, a single target, a pattern's names and every target of
    /// a chain alike, the module's single target alone tagged. The other `assignment` rows and
    /// the `type_alias_statement` row are read off the parsed tree.
    pub(crate) const PYTHON_DEFINITION_TYPES: [(&str, Kind, &str); 9] = [
        ("class_definition", Kind::Type, "Shape"),
        ("function_definition", Kind::Function, "host"),
        ("function_definition", Kind::Method, "area"),
        ("assignment", Kind::Constant, "z"),
        ("assignment", Kind::Constant, "a"),
        ("assignment", Kind::Constant, "y"),
        ("assignment", Kind::Field, "h"),
        ("assignment", Kind::Field, "e"),
        ("type_alias_statement", Kind::Type, "X"),
    ];

    /// The definitions every one of the JS, TS and TSX definitions fixtures holds in its JS
    /// lines, 1-15, as `(line, leaf, kind)`, sorted: each grammar of the family reads them
    /// alike.
    const JS_LINES_DEFINITIONS: [(u32, &str, Kind); 42] = [
        (1, "host", Kind::Function),
        (1, "j", Kind::Constant),
        (1, "k", Kind::Constant),
        (1, "loc", Kind::Constant),
        (1, "v", Kind::Constant),
        (2, "gen", Kind::Function),
        (3, "fexp", Kind::Function),
        (3, "named", Kind::Function),
        (4, "gexp", Kind::Function),
        (4, "gnamed", Kind::Function),
        (5, "g", Kind::Function),
        (6, "Foo", Kind::Type),
        (6, "s", Kind::Field),
        (7, "Bar", Kind::Type),
        (7, "Baz", Kind::Type),
        (8, "lazy", Kind::Function),
        (8, "y", Kind::Constant),
        (9, "a", Kind::Constant),
        (9, "c", Kind::Constant),
        (9, "d", Kind::Constant),
        (9, "r", Kind::Constant),
        (10, "x", Kind::Constant),
        (10, "y", Kind::Constant),
        (10, "z", Kind::Constant),
        (11, "#hid", Kind::Field),
        (11, "#m", Kind::Method),
        (11, "1", Kind::Method),
        (11, "3", Kind::Field),
        (11, "Shape", Kind::Type),
        (11, "a-b", Kind::Method),
        (11, "act", Kind::Method),
        (11, "area", Kind::Method),
        (11, "cb", Kind::Method),
        (11, "constructor", Kind::Constructor),
        (11, "field", Kind::Field),
        (11, "local", Kind::Constant),
        (11, "q-r", Kind::Field),
        (12, "handler", Kind::Function),
        (13, "o2", Kind::Constant),
        (13, "p", Kind::Function),
        (14, "ex", Kind::Constant),
        (15, "el", Kind::Constant),
    ];

    /// The references in the JS lines of every JS and TS definitions fixture, each with its
    /// caller, as `(line, name, caller)`, sorted.
    const JS_LINES_REFERENCES: [(u32, &str, Option<&str>); 9] = [
        (1, "probe", Some("host")),
        (1, "use", Some("host")),
        (5, "spin", Some("g")),
        (6, "tick", Some("Foo.s")),
        (10, "f", None),
        (11, "init", Some("Shape::constructor")),
        (11, "make", Some("Shape::constructor")),
        (11, "paint", Some("Shape::area")),
        (12, "run", Some("handler")),
    ];

    /// The definitions the TS and TSX definitions fixtures hold in their TS lines, 16-23, as
    /// `(line, leaf, kind)`, sorted.
    const TS_LINES_DEFINITIONS: [(u32, &str, Kind); 27] = [
        (16, "sig", Kind::Declaration),
        (17, "2", Kind::Field),
        (17, "4", Kind::Declaration),
        (17, "Shape2", Kind::Type),
        (17, "bare", Kind::Field),
        (17, "m", Kind::Declaration),
        (17, "p-q", Kind::Field),
        (17, "prop", Kind::Field),
        (17, "s-m", Kind::Declaration),
        (18, "5", Kind::Declaration),
        (18, "Abs", Kind::Type),
        (18, "act2", Kind::Method),
        (18, "fld", Kind::Field),
        (18, "run", Kind::Declaration),
        (18, "s-a", Kind::Declaration),
        (18, "s-t", Kind::Field),
        (19, "6", Kind::Variant),
        (19, "Blue", Kind::Variant),
        (19, "Color", Kind::Type),
        (19, "Dark-red", Kind::Variant),
        (19, "Red", Kind::Variant),
        (19, "e-f", Kind::Variant),
        (20, "Alias", Kind::Type),
        (21, "x", Kind::Module),
        (22, "A.B", Kind::Module),
        (23, "X", Kind::Constant),
        (24, "M.N", Kind::Module),
    ];

    /// The references in the TS lines of the TS and TSX definitions fixtures, each with its
    /// caller, as `(line, name, caller)`, sorted.
    const TS_LINES_REFERENCES: [(u32, &str, Option<&str>); 11] = [
        (16, "Arg", None),
        (16, "Ret", None),
        (17, "PQ", Some("Shape2.p-q")),
        (17, "PType", Some("Shape2.prop")),
        (17, "Ret", Some("Shape2")),
        (17, "Two", Some("Shape2.2")),
        (17, "V", Some("Shape2")),
        (18, "FT", Some("Abs.fld")),
        (19, "mk", Some("Color::Blue")),
        (21, "boot", Some("x")),
        (22, "setup", Some("A.B")),
    ];

    /// Rust's declared non-definitions (spec 116): the named node types of the Rust grammar
    /// that THE RULE classifies no definition. An extern block, `foreign_mod_item`, carries no
    /// `name` field and is listed as the spec's table lists it, a container like a block.
    const RUST_NON_DEFINITIONS: &[&str] = &[
        "const_parameter",
        "lifetime_parameter",
        "type_parameter",
        "field_pattern",
        "token_binding_pattern",
        "scoped_identifier",
        "scoped_type_identifier",
        "struct_expression",
        "type_binding",
        "foreign_mod_item",
    ];

    /// Go's declared non-definitions (spec 116); `package_clause`, a container name, carries no
    /// `name` field and is listed as the spec's table lists it.
    const GO_NON_DEFINITIONS: &[&str] = &[
        "package_clause",
        "import_spec",
        "parameter_declaration",
        "variadic_parameter_declaration",
        "type_parameter_declaration",
        "qualified_type",
    ];

    /// C#'s declared non-definitions (spec 116).
    const CSHARP_NON_DEFINITIONS: &[&str] = &[
        "accessor_declaration",
        "parameter",
        "type_parameter",
        "catch_declaration",
        "declaration_expression",
        "declaration_pattern",
        "var_pattern",
        "list_pattern",
        "recursive_pattern",
        "tuple_pattern",
        "parenthesized_variable_designation",
        "tuple_element",
        "argument",
        "attribute",
        "attribute_argument",
        "from_clause",
        "alias_qualified_name",
        "qualified_name",
        "member_access_expression",
        "member_binding_expression",
        "bracketed_parameter_list",
        "parameter_list",
        "extern_alias_directive",
        "using_directive",
    ];

    /// JS's declared non-definitions (spec 116).
    const JS_NON_DEFINITIONS: &[&str] = &[
        "export_specifier",
        "import_specifier",
        "jsx_opening_element",
        "jsx_closing_element",
        "jsx_self_closing_element",
    ];

    /// TS's declared non-definitions (spec 116), the TS and TSX grammars' alike as
    /// `TS_DEFINITION_TYPES` is: JS's, its three JSX types the TSX grammar's alone, plus the
    /// TS types.
    const TS_NON_DEFINITIONS: &[&str] = &[
        "export_specifier",
        "import_specifier",
        "jsx_opening_element",
        "jsx_closing_element",
        "jsx_self_closing_element",
        "enum_body",
        "index_signature",
        "mapped_type_clause",
        "generic_type",
        "nested_type_identifier",
        "type_parameter",
        "type_predicate",
        "optional_parameter",
        "required_parameter",
    ];

    /// Python's declared non-definitions (spec 116).
    const PYTHON_NON_DEFINITIONS: &[&str] = &[
        "aliased_import",
        "future_import_statement",
        "import_from_statement",
        "import_statement",
        "default_parameter",
        "typed_default_parameter",
        "keyword_argument",
        "named_expression",
    ];

    /// The node types of the grammar whose `node-types.json` is `node_types`.
    fn node_types(node_types: &str) -> Vec<serde_json::Value> {
        serde_json::from_str(node_types).expect("node-types.json is JSON")
    }

    /// COMPLETENESS IS A GATE (spec 116 criterion 12): the named node types of the grammar
    /// whose `node-types.json` is `node_types` (SCOPE: a `name` among a type's `fields`, its
    /// `named` flag not meant) that do not sit in exactly one list, the type column of
    /// `definitions` or `non_definitions`, in that file's order: a type in neither list, and a
    /// type in both. Only named types are checked, so a listed type the grammar lacks or holds
    /// with no `name` field (a read-off row's) is never one.
    fn named_types_outside_exactly_one_list(
        node_types: &str,
        definitions: &[(&str, Kind, &str)],
        non_definitions: &[&str],
    ) -> Vec<String> {
        self::node_types(node_types)
            .iter()
            .filter(|t| t["fields"].get("name").is_some())
            .map(|t| t["type"].as_str().expect("a node type names its type"))
            .filter(|ty| {
                let defined = definitions.iter().any(|(d, _, _)| d == ty);
                defined == non_definitions.contains(ty)
            })
            .map(str::to_string)
            .collect()
    }

    /// The `expression` subtypes of the grammar whose `node-types.json` is `node_types`,
    /// flattened through `primary_expression`, sorted.
    fn expression_subtypes(node_types: &str) -> Vec<String> {
        let types = self::node_types(node_types);
        let subtypes = |supertype: &str| -> Vec<String> {
            types
                .iter()
                .find(|t| t["type"] == supertype && t["named"] == true)
                .and_then(|t| t["subtypes"].as_array())
                .expect("the grammar holds the supertype")
                .iter()
                .map(|s| s["type"].as_str().unwrap().to_string())
                .collect()
        };
        let mut flattened: Vec<String> = subtypes("expression")
            .into_iter()
            .flat_map(|t| match t.as_str() {
                "primary_expression" => subtypes("primary_expression"),
                _ => vec![t],
            })
            .collect();
        flattened.sort();
        flattened
    }

    /// The constant enumeration of the query registered for `ext` is every `expression` subtype
    /// of its grammar's `node_types`, flattened through `primary_expression`, but a function
    /// expression, an arrow function, a generator function and a class: the node types the
    /// last `value: [..]` alternation of the query lists, its constant pattern composing last.
    fn assert_constant_enumeration(ext: &str, node_types: &str) {
        let query = crate::symbol_index::registered(ext).tags_query;
        let at = query
            .rfind("value: [")
            .expect("the query holds a value alternation");
        let list = &query[at + "value: [".len()..];
        let mut enumerated: Vec<String> = list[..list.find(']').unwrap()]
            .split_whitespace()
            .map(|t| t.trim_matches(['(', ')']).to_string())
            .collect();
        enumerated.sort();
        let expected: Vec<String> = expression_subtypes(node_types)
            .into_iter()
            .filter(|t| {
                !matches!(
                    t.as_str(),
                    "function_expression" | "arrow_function" | "generator_function" | "class"
                )
            })
            .collect();
        assert_eq!(enumerated, expected, ".{ext}");
    }

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
            let entry = crate::symbol_index::registered(ext);
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
    /// what `SymRef.enclosing` holds, is not current and loads as absent, as is one stamped
    /// `ts-tags-v3`, the generation before a definition carried its parent (spec 119, CONTAINS IS
    /// STRUCTURE); one stamped `ts-tags-v4` is current and loads.
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
            (false, false),
            "a ts-tags-v3 index is not current and loads as absent"
        );
        assert_eq!(
            persisted("ts-tags-v4"),
            (true, true),
            "a ts-tags-v4 index is current and loads"
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
    /// the registered Python one; neither source holds a reference, the upstream JS query tagging
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
                    (8, "Base", Some("Shape.Base")),
                    (8, "Coord", Some("Shape.x")),
                    (8, "Gen", Some("Shape.Gen")),
                    (8, "Ptr", Some("Shape.Ptr")),
                    (8, "Qual", Some("Shape.Qual")),
                    (8, "Side", Some("Shape.side")),
                    (8, "int", Some("Shape.Gen")),
                    (9, "Alias", Some("Alias")),
                    (9, "Base", Some("Alias")),
                    (10, "Canvas", Some("Drawer")),
                    (11, "probe", Some("host")),
                    (12, "Shape", Some("Shape::area")),
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
                    (7, "Probe", Some("S.T::Cell")),
                    (7, "Probe", Some("S.T::Cell")),
                    (7, "Probe", Some("S.T::Cell.made")),
                    (10, "Probe", Some("S.T::Foo.Moved")),
                    (11, "Probe", Some("S.T::Foo.Size")),
                    (12, "Probe", Some("S.T::Foo::Foo")),
                    (13, "Probe", Some("S.T::Foo::~Foo")),
                    (14, "Go", Some("S.T::Foo::+")),
                    (14, "Probe", Some("S.T::Foo::+")),
                    (15, "Probe", Some("S.T::Foo::implicit operator int")),
                    (16, "Probe", Some("S.T::Foo::this")),
                    (17, "Probe", Some("S.T::Foo::Run")),
                    (17, "Probe", Some("S.T::Foo::Run::Inner")),
                ],
            );
        /// JS AND TS DEFINITIONS ARE NODES (spec 116 criterion 9), under the registered JS
        /// query: the fixture yields every definition it holds at its own kind and leaf. A
        /// declarator takes its value's kind: a function, named or generator value a function,
        /// a class value, anonymous or named, a type at the declarator's name, the named class
        /// keeping its own, and every other value a constant, a JSX element included, as is a
        /// valueless `let y;`. A declarator named by a pattern makes each identifier it binds a
        /// constant (`a`, `c`, `d` and `r` of `{a, b: c, d = 2, ...r}`; `x`, `y` and `z` of
        /// `[x, [y], , z = 1]`), and so does a `for .. of` declaring one (`k`, `v` and `j`),
        /// while one assigning `q` or `o.p` makes none. A method is a method at its leaf,
        /// private with its `#`, string-named between its quotes and numeric as written, the
        /// computed one none; `constructor` is a constructor; a class field is a field at the
        /// same leaves, the computed one none, and an arrow- or function-valued one a method.
        /// Each row of JS's DEFINITION TYPES names exactly one of them.
        ///
        /// The callers: the calls in a function's local const and loop are the function's,
        /// those in the constructor's body and in its local const the constructor's, a
        /// generator value's the declarator's and a static field initializer's the field's; the
        /// initializer of a pattern declarator is the file's, each binding spanning its own
        /// identifier alone; and `require` is no reference.
        every_js_definition_type_yields_its_definition_at_its_kind:
            crate::symbol_index::assert_definitions_fixture(
                "js",
                JS_DEFINITIONS_SOURCE,
                &JS_DEFINITION_TYPES,
                &JS_LINES_DEFINITIONS,
                &JS_LINES_REFERENCES,
            );
        /// JS AND TS DEFINITIONS ARE NODES (spec 116 criterion 9), under the registered TS
        /// query: the JS lines yield what they yield under the JS query, a type assertion value
        /// a constant, and the TS lines every TS definition at its own kind and leaf: an
        /// ambient function, an interface's method signature and an abstract method each a
        /// declaration, named by an identifier, a string between its quotes or a number as
        /// written; an interface, an abstract class, an enum and a type alias each a type; a
        /// property signature a field, string-named between its quotes, numeric as written and
        /// one named by a string alone included; a class field a field and a function-valued one
        /// a method; an enum member a variant, with or without a value, string-named between its
        /// quotes and, with a value, numeric as written; `declare module "x"` a module at `x`,
        /// `namespace A.B` one at `A.B` and `module M.N` one at `M.N`; and `as const` a constant.
        /// Each row of TS's DEFINITION TYPES names exactly one of them.
        ///
        /// The callers: a property signature's or a field's type reference is that member's; a
        /// signature's types are its interface's, a declaration being no caller, so an ambient
        /// function's are the file's; an index signature's is its interface's; an enum member
        /// value's call is the member's; and a module's and a namespace's body calls are
        /// theirs.
        every_ts_definition_type_yields_its_definition_at_its_kind:
            crate::symbol_index::assert_definitions_fixture(
                "ts",
                TS_DEFINITIONS_SOURCE,
                &TS_DEFINITION_TYPES,
                &[&JS_LINES_DEFINITIONS[..], &TS_LINES_DEFINITIONS[..]].concat(),
                &[&JS_LINES_REFERENCES[..], &TS_LINES_REFERENCES[..]].concat(),
            );
        /// JS AND TS DEFINITIONS ARE NODES (spec 116 criterion 9), under the TSX query `.tsx`
        /// registers: the TSX fixture, the TS one with a JSX element in place of the type
        /// assertion, yields the definitions and references the TS fixture yields under the TS
        /// query, each row of TS's DEFINITION TYPES naming exactly one definition.
        every_tsx_definition_type_yields_its_definition_at_its_kind:
            crate::symbol_index::assert_definitions_fixture(
                "tsx",
                TSX_DEFINITIONS_SOURCE,
                &TS_DEFINITION_TYPES,
                &[&JS_LINES_DEFINITIONS[..], &TS_LINES_DEFINITIONS[..]].concat(),
                &[&JS_LINES_REFERENCES[..], &TS_LINES_REFERENCES[..]].concat(),
            );
        /// PYTHON DEFINITIONS ARE NODES (spec 116 criterion 10), under the registered Python
        /// query: the fixture yields every definition it holds at its own kind and leaf. A def
        /// is a method when its nearest enclosing definition is a class, a decorated one and
        /// one of a class nested in a function included, and a function otherwise, one nested
        /// in a method included. A module-level assignment makes a constant of each identifier
        /// it binds, its one target whatever value the grammar admits (`z = 1`, `u = 1, 2`,
        /// `m = yield`, `n = o += 1`), an annotated name with no value (`w: int`), each
        /// name of a tuple, a parenthesized tuple or a list pattern, starred included (`a, b`,
        /// `(j, q)`, `[c, *d]`), and each target of a chain once (`x = y = f()`); a class-body
        /// one makes a field of each, an annotated name with no value (`h: int`) and a tuple's
        /// names (`e, g`) included. An assignment under `if TYPE_CHECKING:` and an attribute
        /// target (`self.x = 1`) make none, and a type alias is a type at its name, generic
        /// (`type X[T] = int` at `X`) or not (`type Y = int`). Each row of Python's DEFINITION
        /// TYPES names exactly one of them.
        ///
        /// The callers: a call in a function is the function's, one in a function nested in a
        /// method the nested function's, and one in the value of a class-body assignment of one
        /// target that assignment's (`make()` is `k`'s), its field spanning the whole assignment
        /// as a module-level constant of one target does; the value of a pattern or a chained
        /// assignment is the file's, each bound name spanning its own identifier alone.
        every_python_definition_type_yields_its_definition_at_its_kind:
            crate::symbol_index::assert_definitions_fixture(
                "py",
                PYTHON_DEFINITIONS_SOURCE,
                &PYTHON_DEFINITION_TYPES,
                &[
                    (1, "host", Kind::Function),
                    (3, "Local", Kind::Type),
                    (4, "run", Kind::Method),
                    (6, "Shape", Kind::Type),
                    (7, "h", Kind::Field),
                    (8, "e", Kind::Field),
                    (8, "g", Kind::Field),
                    (9, "k", Kind::Field),
                    (10, "area", Kind::Method),
                    (12, "inner", Kind::Function),
                    (15, "deco", Kind::Method),
                    (17, "a", Kind::Constant),
                    (17, "b", Kind::Constant),
                    (18, "c", Kind::Constant),
                    (18, "d", Kind::Constant),
                    (19, "x", Kind::Constant),
                    (19, "y", Kind::Constant),
                    (20, "z", Kind::Constant),
                    (21, "w", Kind::Constant),
                    (24, "X", Kind::Type),
                    (25, "u", Kind::Constant),
                    (26, "m", Kind::Constant),
                    (27, "n", Kind::Constant),
                    (28, "j", Kind::Constant),
                    (28, "q", Kind::Constant),
                    (29, "Y", Kind::Type),
                ],
                &[
                    (2, "probe", Some("host")),
                    (9, "make", Some("Shape.k")),
                    (13, "paint", Some("Shape::area::inner")),
                    (18, "f", None),
                    (19, "f", None),
                ],
            );
        /// Every Python definition carries its extent (spec 119, ONE EXTENT): a single-target
        /// module-level assignment (`z = 1`, `w: int`, and one of each other value type the
        /// grammar admits) is bounded by the whole assignment and every def and class by its
        /// block, while a chained assignment's targets (`x = y = f()`) and every other bound or
        /// read-off name, each read off a one-line range, end on their own line.
        the_python_query_bounds_an_assignment_whole_and_a_chained_target_by_its_own_line: assert_eq!(
            crate::symbol_index::extents_by_line(&crate::symbol_index::extracted(
                "py", PYTHON_DEFINITIONS_SOURCE)),
            [
                (1, 5, "host"),
                (3, 5, "Local"),
                (4, 5, "run"),
                (6, 16, "Shape"),
                (7, 7, "h"),
                (8, 8, "e"),
                (8, 8, "g"),
                (9, 9, "k"),
                (10, 13, "area"),
                (12, 13, "inner"),
                (15, 16, "deco"),
                (17, 17, "a"),
                (17, 17, "b"),
                (18, 18, "c"),
                (18, 18, "d"),
                (19, 19, "x"),
                (19, 19, "y"),
                (20, 20, "z"),
                (21, 21, "w"),
                (24, 24, "X"),
                (25, 25, "u"),
                (26, 26, "m"),
                (27, 27, "n"),
                (28, 28, "j"),
                (28, 28, "q"),
                (29, 29, "Y"),
            ]
        );
        /// The JS query's constant enumeration is the JS grammar's `expression` subtypes,
        /// flattened through `primary_expression`, but the four the function and class
        /// patterns tag, so a declarator of any other value is a constant.
        the_js_constant_enumeration_is_every_other_expression_type:
            assert_constant_enumeration("js", tree_sitter_javascript::NODE_TYPES);
        /// The TS query's constant enumeration is the TS grammar's, its type assertion included.
        the_ts_constant_enumeration_is_every_other_expression_type:
            assert_constant_enumeration("ts", tree_sitter_typescript::TYPESCRIPT_NODE_TYPES);
        /// The TSX query's constant enumeration is the TSX grammar's, its JSX elements
        /// included and no type assertion.
        the_tsx_constant_enumeration_is_every_other_expression_type:
            assert_constant_enumeration("tsx", tree_sitter_typescript::TSX_NODE_TYPES);
        /// COMPLETENESS IS A GATE (spec 116 criterion 12): every named node type of the Rust
        /// grammar sits in exactly one of Rust's lists.
        every_named_rust_type_sits_in_exactly_one_list: assert_eq!(
            named_types_outside_exactly_one_list(
                tree_sitter_rust::NODE_TYPES,
                &RUST_DEFINITION_TYPES,
                RUST_NON_DEFINITIONS
            ),
            Vec::<String>::new()
        );
        /// Every named node type of the Go grammar sits in exactly one of Go's lists.
        every_named_go_type_sits_in_exactly_one_list: assert_eq!(
            named_types_outside_exactly_one_list(
                tree_sitter_go::NODE_TYPES,
                &GO_DEFINITION_TYPES,
                GO_NON_DEFINITIONS
            ),
            Vec::<String>::new()
        );
        /// Every named node type of the C# grammar sits in exactly one of C#'s lists.
        every_named_csharp_type_sits_in_exactly_one_list: assert_eq!(
            named_types_outside_exactly_one_list(
                tree_sitter_c_sharp::NODE_TYPES,
                &CSHARP_DEFINITION_TYPES,
                CSHARP_NON_DEFINITIONS
            ),
            Vec::<String>::new()
        );
        /// Every named node type of the JS grammar sits in exactly one of JS's lists.
        every_named_js_type_sits_in_exactly_one_list: assert_eq!(
            named_types_outside_exactly_one_list(
                tree_sitter_javascript::NODE_TYPES,
                &JS_DEFINITION_TYPES,
                JS_NON_DEFINITIONS
            ),
            Vec::<String>::new()
        );
        /// Every named node type of the TS grammar sits in exactly one of TS's lists.
        every_named_ts_type_sits_in_exactly_one_list: assert_eq!(
            named_types_outside_exactly_one_list(
                tree_sitter_typescript::TYPESCRIPT_NODE_TYPES,
                &TS_DEFINITION_TYPES,
                TS_NON_DEFINITIONS
            ),
            Vec::<String>::new()
        );
        /// Every named node type of the TSX grammar sits in exactly one of TS's lists, its
        /// JSX types included.
        every_named_tsx_type_sits_in_exactly_one_list: assert_eq!(
            named_types_outside_exactly_one_list(
                tree_sitter_typescript::TSX_NODE_TYPES,
                &TS_DEFINITION_TYPES,
                TS_NON_DEFINITIONS
            ),
            Vec::<String>::new()
        );
        /// Every named node type of the Python grammar sits in exactly one of Python's lists.
        every_named_python_type_sits_in_exactly_one_list: assert_eq!(
            named_types_outside_exactly_one_list(
                tree_sitter_python::NODE_TYPES,
                &PYTHON_DEFINITION_TYPES,
                PYTHON_NON_DEFINITIONS
            ),
            Vec::<String>::new()
        );
        /// The gate fails on a named type absent from both lists and on one in both, in the
        /// grammar's order, and passes a type in one list alone, a type with no `name` field
        /// and a type with no fields at all, whatever its `named` flag says.
        the_gate_fails_on_a_named_type_in_neither_list_or_in_both: assert_eq!(
            named_types_outside_exactly_one_list(
                r#"[
                    {"type": "defined", "named": true, "fields": {"name": {}}},
                    {"type": "unlisted", "named": true, "fields": {"name": {}}},
                    {"type": "declared", "named": true, "fields": {"name": {}}},
                    {"type": "both", "named": true, "fields": {"name": {}}},
                    {"type": "nameless", "named": true, "fields": {"body": {}}},
                    {"type": "fieldless", "named": true},
                    {"type": "unflagged", "named": false, "fields": {"name": {}}}
                ]"#,
                &[("defined", Kind::Type, "D"), ("both", Kind::Type, "B")],
                &["declared", "both"]
            ),
            ["unlisted", "both", "unflagged"]
        );
    }
}
