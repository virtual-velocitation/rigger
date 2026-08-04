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
            });
        } else {
            ref_positions.push(tag.range.start);
            refs.push(SymRef {
                name,
                line,
                enclosing: None,
            });
        }
    }
    // Attribute each reference to the innermost enclosing definition (the caller, spec 37). The
    // reference order is unchanged - `enclosing` is a derived per-reference attribute, never a new
    // sort key, so identical source still yields byte-identical downstream events.
    for (r, &pos) in refs.iter_mut().zip(ref_positions.iter()) {
        r.enclosing = enclosing_def(&def_ranges, pos);
    }
    Ok(FileSymbols { lang, defs, refs })
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
