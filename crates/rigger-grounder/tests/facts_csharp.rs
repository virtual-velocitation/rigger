//! C# STAGES ITS ROW (spec 117, OTHER LANGUAGES): the facts the bundled C# grammar gives a file
//! hold an `impl` mention for each entry of a base list, a `type` mention for an identifier or
//! generic name in type position, a `field` mention for a member access that is not called,
//! qualified by a capitalised object, while a called one stays a call, and a definition's doc
//! span from the `///` block above it, read back without its XML tags.

#![cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]

extern crate rigger_grounder as rigger;

#[path = "../../../tests/common/symbol_index.rs"]
mod symbol_index;

use rigger_domain::code_facts::Mention;
use rigger_domain::code_view::{Entry, Extract};
use rigger_domain::contextgraph::{REF_KIND_FIELD, REF_KIND_IMPL, REF_KIND_TYPE};
use rigger_grounder::facts::FactsExtractor;
use symbol_index::mention_in;

/// A case: what it shows, the C# source, the mentions its facts hold, whole and in order, and
/// each definition's leaf with its doc span and the span's text read back, in order.
type Case = (
    &'static str,
    &'static str,
    Vec<Mention>,
    Vec<(&'static str, Option<(u32, u32)>, &'static str)>,
);

/// C# STAGES ITS ROW: each entry of a class's, struct's, record's or interface's base list is an
/// `impl` mention, a record's primary-constructor base included and an enum's base none; an
/// identifier or generic name in a property, return, parameter, local, array element, nullable,
/// cast, `is`, `as`, `typeof`, generic argument or field type is a `type` mention, a generic
/// name's type arguments each a mention too, a qualified type none; `new T()` stays a call; `x.f`
/// not called, read or assigned, is a `field` mention, named `Square.SIDES` when a capitalised
/// name qualifies it, while a called `x.g()` is a call; and the `///` block directly above a
/// definition, a blank line and a bare `///` line inside it included, is its doc span, which
/// `doc_text` reads back without its XML tags, while a `////` line, a block comment and a run a
/// plain `//` line joins give none, over the production extractor.
#[test]
fn csharp_stages_its_row() {
    // A mention inside the definition at index `at`, written `name` of `leaf`.
    let in_def = |at: u32, name: &str, leaf: &str, kind: Option<&str>, line: u32| {
        mention_in(Some(at), name, leaf, kind, line, false)
    };
    // A mention inside the definition at index `at`, named by its leaf.
    let leaf_in = |at: u32, leaf: &str, kind: Option<&str>, line: u32| {
        mention_in(Some(at), leaf, leaf, kind, line, false)
    };
    let cases: [Case; 7] = [
        (
            "a base list, a parameter's types, a member constant, an assigned member and a doc block",
            "/// <summary>A square.</summary>\n/// <remarks>Its <c>Side</c> is fixed.</remarks>\nclass Square : IShape {\n    const int SIDES = 4;\n    int Side;\n    void Draw(List<IShape> s) { var n = Square.SIDES; s.Side = 2; }\n}\n",
            vec![
                leaf_in(0, "IShape", Some(REF_KIND_IMPL), 3),
                leaf_in(3, "IShape", Some(REF_KIND_TYPE), 6),
                leaf_in(3, "List", Some(REF_KIND_TYPE), 6),
                in_def(3, "Square.SIDES", "SIDES", Some(REF_KIND_FIELD), 6),
                leaf_in(3, "Side", Some(REF_KIND_FIELD), 6),
            ],
            vec![
                ("Square", Some((1, 2)), "A square.\nIts Side is fixed."),
                ("SIDES", None, ""),
                ("Side", None, ""),
                ("Draw", None, ""),
            ],
        ),
        (
            "each type position, and a qualified type",
            "class K {\n    Prop P { get; }\n    Ret M(Param p) {\n        Local l = null;\n        Elem[] a = null;\n        Nul? n = null;\n        var c = (Cast)p;\n        var i = p is Is;\n        var j = p is Decl t;\n        var k = p as As;\n        var y = typeof(Type);\n        Gen<Arg> g = null;\n        Ns.Qual q = null;\n        global::Glob r = null;\n        return null;\n    }\n    Fld f;\n}\n",
            vec![
                leaf_in(1, "Prop", Some(REF_KIND_TYPE), 2),
                leaf_in(2, "Ret", Some(REF_KIND_TYPE), 3),
                leaf_in(2, "Param", Some(REF_KIND_TYPE), 3),
                leaf_in(2, "Local", Some(REF_KIND_TYPE), 4),
                leaf_in(2, "Elem", Some(REF_KIND_TYPE), 5),
                leaf_in(2, "Nul", Some(REF_KIND_TYPE), 6),
                leaf_in(2, "Cast", Some(REF_KIND_TYPE), 7),
                leaf_in(2, "Is", Some(REF_KIND_TYPE), 8),
                leaf_in(2, "Decl", Some(REF_KIND_TYPE), 9),
                leaf_in(2, "As", Some(REF_KIND_TYPE), 10),
                leaf_in(2, "Type", Some(REF_KIND_TYPE), 11),
                leaf_in(2, "Arg", Some(REF_KIND_TYPE), 12),
                leaf_in(2, "Gen", Some(REF_KIND_TYPE), 12),
                leaf_in(0, "Fld", Some(REF_KIND_TYPE), 17),
            ],
            vec![("K", None, ""), ("P", None, ""), ("M", None, ""), ("f", None, "")],
        ),
        (
            "a struct's, a record's and an interface's base list, and an enum's base",
            "struct S : IS { }\nrecord R(int X) : Base(X), IR;\ninterface I : IBase<Box> { }\nenum E : byte { A }\n",
            vec![
                leaf_in(0, "IS", Some(REF_KIND_IMPL), 1),
                leaf_in(1, "Base", Some(REF_KIND_IMPL), 2),
                leaf_in(1, "IR", Some(REF_KIND_IMPL), 2),
                leaf_in(2, "Box", Some(REF_KIND_TYPE), 3),
                leaf_in(2, "IBase", Some(REF_KIND_IMPL), 3),
            ],
            vec![
                ("S", None, ""),
                ("R", None, ""),
                ("I", None, ""),
                ("E", None, ""),
                ("A", None, ""),
            ],
        ),
        (
            "a constructed generic type and a constructed type",
            "class K {\n    void M() {\n        var m = new List<Item>();\n        var z = new Box();\n    }\n}\n",
            vec![
                leaf_in(1, "Item", Some(REF_KIND_TYPE), 3),
                leaf_in(1, "List", None, 3),
                leaf_in(1, "Box", None, 4),
            ],
            vec![("K", None, ""), ("M", None, "")],
        ),
        (
            "a member access called and not, in a chain, in arguments, capitalised and on this",
            "class K {\n    void M() {\n        a.b.c();\n        x.g(x.h);\n        Square.Make();\n        this.f = 1;\n        Ns.Kit.Run();\n    }\n}\n",
            vec![
                leaf_in(1, "b", Some(REF_KIND_FIELD), 3),
                leaf_in(1, "c", None, 3),
                leaf_in(1, "g", None, 4),
                leaf_in(1, "h", Some(REF_KIND_FIELD), 4),
                in_def(1, "Square.Make", "Make", None, 5),
                leaf_in(1, "f", Some(REF_KIND_FIELD), 6),
                in_def(1, "Ns.Kit", "Kit", Some(REF_KIND_FIELD), 7),
                leaf_in(1, "Run", None, 7),
            ],
            vec![("K", None, ""), ("M", None, "")],
        ),
        (
            "a doc block across a blank line, a four-slash line, a run a plain line joins, a block comment and a marker with no space",
            "class K {\n    /// a doc\n\n    /// still a.\n    int A;\n    //// not doc\n    int B;\n    // plain\n    /// doc c\n    int C; // trailing\n    /// doc d\n    int D;\n    /* block */\n    int E;\n    ///no space\n    const int F = 1;\n}\n",
            vec![],
            vec![
                ("K", None, ""),
                ("A", Some((2, 4)), "a doc\nstill a."),
                ("B", None, ""),
                ("C", None, ""),
                ("D", None, ""),
                ("E", None, ""),
                ("F", Some((15, 15)), "no space"),
            ],
        ),
        (
            "a generic name as a return, after `is` and `as` and inside a type argument, and a doc block opening on a bare `///`",
            "class K {\n    GRet<A> M(object p) {\n        var i = p is GIs<B>;\n        var k = p as GAs<C>;\n        var l = p is List<Box<D>>;\n        return null;\n    }\n    ///\n    /// bare above\n    int F;\n}\n",
            vec![
                leaf_in(1, "A", Some(REF_KIND_TYPE), 2),
                leaf_in(1, "GRet", Some(REF_KIND_TYPE), 2),
                leaf_in(1, "B", Some(REF_KIND_TYPE), 3),
                leaf_in(1, "GIs", Some(REF_KIND_TYPE), 3),
                leaf_in(1, "C", Some(REF_KIND_TYPE), 4),
                leaf_in(1, "GAs", Some(REF_KIND_TYPE), 4),
                leaf_in(1, "D", Some(REF_KIND_TYPE), 5),
                leaf_in(1, "Box", Some(REF_KIND_TYPE), 5),
                leaf_in(1, "List", Some(REF_KIND_TYPE), 5),
            ],
            vec![
                ("K", None, ""),
                ("M", None, ""),
                ("F", Some((8, 9)), "bare above"),
            ],
        ),
    ];
    for (case, source, mentions, docs) in cases {
        let entry = Entry {
            path: "a.cs".to_string(),
            blob: String::new(),
        };
        let facts = FactsExtractor.extract(&entry, source.as_bytes());
        assert_eq!(facts.mentions, mentions, "{case}: the mentions");
        // A doc span read back: its lines of the source, joined, through the C# doc rule.
        let read_back = |span: Option<(u32, u32)>| match span {
            None => String::new(),
            Some((start, end)) => {
                let lines = source.lines().skip(start as usize - 1);
                let raw = lines.take((end - start + 1) as usize).collect::<Vec<_>>();
                FactsExtractor.doc_text("csharp", &raw.join("\n"))
            }
        };
        assert_eq!(
            facts
                .defs
                .iter()
                .map(|d| (d.leaf.as_str(), d.doc, read_back(d.doc)))
                .collect::<Vec<_>>(),
            docs.into_iter()
                .map(|(leaf, span, text)| (leaf, span, text.to_string()))
                .collect::<Vec<_>>(),
            "{case}: each definition's doc span and its text"
        );
    }
}
