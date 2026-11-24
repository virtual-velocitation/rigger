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
/// `impl` mention, a record's primary-constructor base included, a comment inside the list and
/// an enum's base none; an identifier or generic name in a property, return, parameter, local,
/// array element, nullable, cast, `is`, `as`, `typeof`, generic argument or field type, or in a
/// constant pattern under a `not`, an `and`, an `or` or parentheses, two pairs included, a switch
/// case or arm and a property subpattern, is a `type` mention, a generic name's type arguments
/// each a mention too, while a qualified type is none and a qualified name in a constant pattern,
/// a member access to the grammar, is a `field` mention; a positional pattern's type,
/// `p is Pt(0, 0)`, an invocation in a constant pattern to the grammar, is a `type` mention after
/// `is`, under a `not`, in a switch case or arm and in one pair of parentheses, and so is one with a
/// declaration first in a switch case label, `case Pt(var x, 0):`, stacked included, an invocation
/// in the switch section to the grammar, while a call in a case's body or `when` is none and one
/// in two pairs is none; `nameof` in any pattern position, a switch case, with `when`, `or` or
/// `and not` and under `not`, a switch arm, a property subpattern, a list, parentheses and a
/// declaring `nameof(var x)` included, is none, while a member access in its argument is the
/// `field` mention it names, as in an expression; `new T()` stays a call; `x.f` not called, read or
/// assigned, is a `field` mention, named `Square.SIDES` when a capitalised name qualifies it,
/// while a called `x.g()` is a call; and the `///` block directly above a definition, a blank
/// line and a bare `///` line inside it included, under at most two other comments, a plain line
/// and a trailing comment included, is its doc span, which `doc_text` reads back without its XML
/// tags, while a `////` line, a block comment, a block under three other comments and a `///`
/// line a plain one follows give none, over the production extractor.
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
    let cases: [Case; 16] = [
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
            "a doc block across a blank line, a four-slash line, a block under a plain line and under a trailing comment, a block comment and a marker with no space",
            "class K {\n    /// a doc\n\n    /// still a.\n    int A;\n    //// not doc\n    int B;\n    // plain\n    /// doc c\n    int C; // trailing\n    /// doc d\n    int D;\n    /* block */\n    int E;\n    ///no space\n    const int F = 1;\n}\n",
            vec![],
            vec![
                ("K", None, ""),
                ("A", Some((2, 4)), "a doc\nstill a."),
                ("B", None, ""),
                ("C", Some((9, 9)), "doc c"),
                ("D", Some((11, 11)), "doc d"),
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
        (
            "a doc block under two other comments and under three, a block a plain line splits and a `///` line a plain one follows",
            "class K {\n    // p1\n    // p2\n    /// d1\n    /// d2\n    int A;\n    // q1\n    // q2\n    // q3\n    /// e1\n    int B;\n    /// old\n    // plain\n    /// new\n    int C;\n    /// f1\n    // f2\n    int D;\n}\n",
            vec![],
            vec![
                ("K", None, ""),
                ("A", Some((4, 5)), "d1\nd2"),
                ("B", None, ""),
                ("C", Some((14, 14)), "new"),
                ("D", None, ""),
            ],
        ),
        (
            "a line comment and a block comment inside a class's, struct's, interface's and record's base list",
            "class A : IOne, // see Shape.IDraw\n    ITwo { }\nstruct S : /* Ns::Q */ IS { }\ninterface I : /* c */ IBase { }\nrecord R : // r\n    RBase;\n",
            vec![
                leaf_in(0, "IOne", Some(REF_KIND_IMPL), 1),
                leaf_in(0, "ITwo", Some(REF_KIND_IMPL), 2),
                leaf_in(1, "IS", Some(REF_KIND_IMPL), 3),
                leaf_in(2, "IBase", Some(REF_KIND_IMPL), 4),
                leaf_in(3, "RBase", Some(REF_KIND_IMPL), 6),
            ],
            vec![
                ("A", None, ""),
                ("S", None, ""),
                ("I", None, ""),
                ("R", None, ""),
            ],
        ),
        (
            "a constant pattern under `not`, `or`, parentheses and `and`, in a switch case and arm and in a property subpattern",
            "class K {\n    bool M(object p) {\n        var a = p is not NotT;\n        var b = p is OrA or OrB;\n        var c = p is (ParT);\n        var d = p is not (AndA and GAnd<GArg>);\n        switch (p) { case SwT: break; }\n        var e = p switch { ArmT => 1, _ => 0 };\n        return p is { Prop: SubT };\n    }\n}\n",
            vec![
                leaf_in(1, "NotT", Some(REF_KIND_TYPE), 3),
                leaf_in(1, "OrA", Some(REF_KIND_TYPE), 4),
                leaf_in(1, "OrB", Some(REF_KIND_TYPE), 4),
                leaf_in(1, "ParT", Some(REF_KIND_TYPE), 5),
                leaf_in(1, "AndA", Some(REF_KIND_TYPE), 6),
                leaf_in(1, "GArg", Some(REF_KIND_TYPE), 6),
                leaf_in(1, "GAnd", Some(REF_KIND_TYPE), 6),
                leaf_in(1, "SwT", Some(REF_KIND_TYPE), 7),
                leaf_in(1, "ArmT", Some(REF_KIND_TYPE), 8),
                leaf_in(1, "SubT", Some(REF_KIND_TYPE), 9),
            ],
            vec![("K", None, ""), ("M", None, "")],
        ),
        (
            "a qualified name after `is` and in a switch case, which the grammar parses as a member access",
            "class K {\n    void M(object p) {\n        var f = p is Ns.QualT;\n        switch (p) { case Color.Red: break; }\n    }\n}\n",
            vec![
                in_def(1, "Ns.QualT", "QualT", Some(REF_KIND_FIELD), 3),
                in_def(1, "Color.Red", "Red", Some(REF_KIND_FIELD), 4),
            ],
            vec![("K", None, ""), ("M", None, "")],
        ),
        (
            "a positional pattern after `is`, under `not`, in a switch case and arm and in parentheses, and `nameof` after `is`",
            "class K {\n    int M(object p, string s) {\n        var a = p is Pt(0, 0);\n        var b = p is not PtN(0, 0);\n        switch (p) { case PtD(0, 0): break; }\n        var c = p switch { PtA(0, 0) => 1, _ => 0 };\n        var d = p is (PtP(0, 0));\n        return s is nameof(Foo) ? 1 : 0;\n    }\n}\n",
            vec![
                leaf_in(1, "Pt", Some(REF_KIND_TYPE), 3),
                leaf_in(1, "PtN", Some(REF_KIND_TYPE), 4),
                leaf_in(1, "PtD", Some(REF_KIND_TYPE), 5),
                leaf_in(1, "PtA", Some(REF_KIND_TYPE), 6),
                leaf_in(1, "PtP", Some(REF_KIND_TYPE), 7),
            ],
            vec![("K", None, ""), ("M", None, "")],
        ),
        (
            "an identifier and a positional pattern in two pairs of parentheses, the second a nested expression to the grammar",
            "class K {\n    void M(object p) {\n        var a = p is ((ParQ));\n        var b = p is ((PtQ(0, 0)));\n    }\n}\n",
            vec![leaf_in(1, "ParQ", Some(REF_KIND_TYPE), 3)],
            vec![("K", None, ""), ("M", None, "")],
        ),
        (
            "a positional pattern with a declaration first in a switch case label, stacked included, an invocation in the switch section to the grammar, and a call in a case's body or `when`",
            "class K {\n    void M1(object p) { switch (p) { case Pt(var x, var y): break; } }\n    void M2(object p) { switch (p) { case Pt(var x, 0): break; } }\n    void M3(object p) { switch (p) { case Pt(int u, int v): break; } }\n    void M4(object p) { switch (p) { case Pt(_, var v): break; } }\n    void M5(object p) { switch (p) { case Pt(int u, _): break; } }\n    void M6(object p) { switch (p) { case PtA(var u, var v): case PtB(var w, var z): break; } }\n    void M7(object p) { switch (p) { case 0: Go(1); break; case int i when Check(i): break; } }\n}\n",
            vec![
                leaf_in(1, "Pt", Some(REF_KIND_TYPE), 2),
                leaf_in(2, "Pt", Some(REF_KIND_TYPE), 3),
                leaf_in(3, "Pt", Some(REF_KIND_TYPE), 4),
                leaf_in(4, "Pt", Some(REF_KIND_TYPE), 5),
                leaf_in(5, "Pt", Some(REF_KIND_TYPE), 6),
                leaf_in(6, "PtA", Some(REF_KIND_TYPE), 7),
                leaf_in(6, "PtB", Some(REF_KIND_TYPE), 7),
            ],
            vec![
                ("K", None, ""),
                ("M1", None, ""),
                ("M2", None, ""),
                ("M3", None, ""),
                ("M4", None, ""),
                ("M5", None, ""),
                ("M6", None, ""),
                ("M7", None, ""),
            ],
        ),
        (
            "`nameof` in a switch case, with `when`, `or`, `and not` or under `not`, a switch arm, a property subpattern, a list and parentheses, and the grammar's reading of a declaring `nameof(var x)` in a case and after `is`",
            "class K {\n    int M(object p, string s, bool ok) {\n        switch (s) { case nameof(Foo): break; }\n        switch (s) { case nameof(Foo) when ok: break; }\n        switch (s) { case nameof(Foo) or nameof(Bar): break; }\n        switch (s) { case nameof(Foo) and not nameof(Bar): break; }\n        switch (s) { case not nameof(Foo): break; }\n        var a = s switch { nameof(Foo) => 1, _ => 0 };\n        var b = s switch { nameof(Foo) or nameof(Bar) => 1, _ => 0 };\n        var c = s switch { nameof(Foo) when ok => 1, _ => 0 };\n        var d = s switch { not nameof(Foo) => 1, _ => 0 };\n        var e = p is { Name: nameof(Foo) };\n        var f = p is [nameof(Foo), ..];\n        var g = s is (not nameof(Foo));\n        switch (s) { case nameof(var x): break; }\n        var h = s is nameof(var y);\n        return 0;\n    }\n}\n",
            vec![],
            vec![("K", None, ""), ("M", None, "")],
        ),
        (
            "a member access in `nameof`'s argument in a switch case and in an expression",
            "class K {\n    void M(string s) {\n        switch (s) { case nameof(Ns.Foo): break; }\n        var n = nameof(Ns.Bar);\n    }\n}\n",
            vec![
                in_def(1, "Ns.Foo", "Foo", Some(REF_KIND_FIELD), 3),
                in_def(1, "Ns.Bar", "Bar", Some(REF_KIND_FIELD), 4),
            ],
            vec![("K", None, ""), ("M", None, "")],
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
