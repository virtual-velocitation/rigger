//! The grammar registry (architecture 5.5.3): the `extension -> (grammar, tag query)` table
//! that decides WHICH grammar a file is parsed under. It supplies the injected pair unit 1's
//! `extract` consumes; it never parses anything itself. Adding a language is adding one match
//! arm here - no bespoke per-language code. Confined to the `symbols` feature because it names
//! `tree_sitter::Language`; the parser-free model stays in the light lane.

use crate::grounder::symbols::model::Lang;
use std::sync::OnceLock;

/// The version of the grammar / tag-query set this build indexes with (architecture 5.5.3),
/// stamped into unit 3's `BlastRadiusComputed` audit event (via `Symbols::index_stamp`) so a
/// recorded radius names the tag-query generation that produced it. Bump it when a shipped
/// grammar or an authored tags query changes, so a radius computed under an older grammar set
/// is distinguishable on replay from one the current set would produce.
pub const GRAMMAR_TAGS_VERSION: &str = "ts-tags-v1";

/// The C# `tags` query. The upstream `tree-sitter-c-sharp` crate ships a `tags.scm` whose last
/// pattern captures a bare `@module`, which `tree-sitter-tags` rejects (it accepts only
/// `@definition.*` / `@reference.*` / `@name` / `@doc` / `@local.*`), so the shipped query fails
/// to compile. That pattern is also redundant: the line above it already tags the same
/// `namespace_declaration` as `@definition.module`. This is the shipped query verbatim minus
/// that one invalid, redundant line - authored here per the spec's "author or verify each
/// grammar's tag query" mandate.
const CSHARP_TAGS: &str = r#"
(class_declaration name: (identifier) @name) @definition.class

(class_declaration (base_list (_) @name)) @reference.class

(interface_declaration name: (identifier) @name) @definition.interface

(interface_declaration (base_list (_) @name)) @reference.interface

(method_declaration name: (identifier) @name) @definition.method

(object_creation_expression type: (identifier) @name) @reference.class

(type_parameter_constraints_clause (identifier) @name) @reference.class

(type_parameter_constraint (type type: (identifier) @name)) @reference.class

(variable_declaration type: (identifier) @name) @reference.class

(invocation_expression function: (member_access_expression name: (identifier) @name)) @reference.send

(namespace_declaration name: (identifier) @name) @definition.module
"#;

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
    Q.get_or_init(|| {
        format!(
            "{}\n{}",
            tree_sitter_javascript::TAGS_QUERY,
            tree_sitter_typescript::TAGS_QUERY
        )
    })
    .as_str()
}

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
    let (lang, language, tags_query): (Lang, tree_sitter::Language, &'static str) = match ext {
        "rs" => (
            Lang::Rust,
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::TAGS_QUERY,
        ),
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
            tree_sitter_javascript::TAGS_QUERY,
        ),
        "go" => (
            Lang::Go,
            tree_sitter_go::LANGUAGE.into(),
            tree_sitter_go::TAGS_QUERY,
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
mod tests {
    use super::*;
    use crate::grounder::symbols::extract::extract;
    use crate::grounder::symbols::model::Lang;

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
        let tsx = for_extension("tsx").unwrap();
        // A `.tsx` component with a JSX body still yields its function definition (the plain TS
        // grammar would error on the JSX).
        let src = "function Panel() { return <div>{title}</div>; }\n";
        let fs = extract(src, tsx.lang, &tsx.language, tsx.tags_query).unwrap();
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
}
