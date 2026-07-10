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
            defs.push(Def {
                kind: kind_of(syntax),
                name,
                line,
            });
        } else {
            refs.push(SymRef { name, line });
        }
    }
    Ok(FileSymbols { lang, defs, refs })
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
}
