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
/// `is`, under a `not`, in a switch case or arm and in one pair of parentheses, and so is one in a
/// switch case label with no `when` whose first argument declares one variable or discards,
/// `case Pt(var x, 0):`, stacked included, an invocation in the switch section to the grammar,
/// while a call in a case's body or `when` is none and one in two pairs is none; `nameof` in any
/// pattern position, a switch case, with `when`, `or` or `and not` and under `not`, a switch arm,
/// a property subpattern, a list, parentheses and a declaring `nameof(var x)` included, is none,
/// while a member access in its argument is the `field` mention it names, as in an expression;
/// `new T()` stays a call; `x.f` not called, read or assigned, is a `field` mention, named
/// `Square.SIDES` when a capitalised name qualifies it, while a called `x.g()` is a call; and the
/// `///` block directly above a definition, a blank line and a bare `///` line inside it
/// included, under at most two other comments, a plain line and a trailing comment included, is
/// its doc span, which `doc_text` reads back without its XML tags, while a `////` line, a block
/// comment, a block under three other comments and a `///` line a plain one follows give none;
/// and each per-form row, one pattern form written as a statement in a method's body, in each
/// position and under each argument mix, a list case label and a list in one pair of parentheses
/// included, stages the `type`, `field` or call mentions, or none, that the C# reference
/// patterns' doc (`CSHARP_REFERENCE_TAGS`) states for that form; over the production extractor.
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
    // A source whose one statement, `$stmt`, is the body of `K.M` on line 3, before its return.
    macro_rules! in_m {
        ($stmt:literal) => {
            concat!(
                "class K {\n    int M(object p, string s, bool ok) {\n        ",
                $stmt,
                "\n        return 0;\n    }\n}\n"
            )
        };
    }
    // The definitions of an `in_m!` source, neither documented.
    let k_m = || vec![("K", None, ""), ("M", None, "")];
    // A `type` mention in `K.M` on line 3, named by its leaf.
    let ty = |leaf: &str| leaf_in(1, leaf, Some(REF_KIND_TYPE), 3);
    // A `field` mention in `K.M` on line 3, written `name` of `leaf`.
    let field = |name: &str, leaf: &str| in_def(1, name, leaf, Some(REF_KIND_FIELD), 3);
    // A call in `K.M` on line 3, written `name` of `leaf`.
    let call = |name: &str, leaf: &str| in_def(1, name, leaf, None, 3);
    let cases: [Case; 188] = [
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
            "a positional pattern whose first argument declares one variable or discards in a switch case label with no `when`, stacked included, an invocation in the switch section to the grammar, and a call in a case's body or `when`",
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
        (
            "an array type after `is`",
            in_m!("var r = p is Ta[];"),
            vec![ty("Ta")],
            k_m(),
        ),
        (
            "a nullable type after `is`",
            in_m!("var r = p is Tb?;"),
            vec![ty("Tb")],
            k_m(),
        ),
        (
            "an array type pattern in a switch case",
            in_m!("switch (p) { case Tc[]: break; }"),
            vec![ty("Tc")],
            k_m(),
        ),
        (
            "an array type pattern in a switch arm",
            in_m!("var r = p switch { Td[] => 1, _ => 0 };"),
            vec![ty("Td")],
            k_m(),
        ),
        (
            "an array type pattern under `not`",
            in_m!("var r = p is not Tm[];"),
            vec![ty("Tm")],
            k_m(),
        ),
        (
            "a predefined type in a switch arm",
            in_m!("var r = p switch { int => 1, _ => 0 };"),
            vec![],
            k_m(),
        ),
        (
            "a generic name and its type argument in a switch arm",
            in_m!("var r = p switch { Ti<Tj> => 1, _ => 0 };"),
            vec![ty("Tj"), ty("Ti")],
            k_m(),
        ),
        (
            "a generic name with a predefined type argument after `is`",
            in_m!("var r = p is Cm<int>;"),
            vec![ty("Cm")],
            k_m(),
        ),
        (
            "two identifiers under `or` in a switch arm",
            in_m!("var r = p switch { Tk or Tl => 1, _ => 0 };"),
            vec![ty("Tk"), ty("Tl")],
            k_m(),
        ),
        (
            "a member access on a lower-case object in a switch case, the field of its leaf",
            in_m!("switch (p) { case ck.Cl: break; }"),
            vec![field("Cl", "Cl")],
            k_m(),
        ),
        (
            "a cast after `is`",
            in_m!("var r = p is (Ca)0;"),
            vec![ty("Ca")],
            k_m(),
        ),
        (
            "a cast in a switch case",
            in_m!("switch (p) { case (Cb)0: break; }"),
            vec![ty("Cb")],
            k_m(),
        ),
        (
            "a `default` expression's type after `is`",
            in_m!("var r = p is default(Cc);"),
            vec![ty("Cc")],
            k_m(),
        ),
        (
            "a `typeof` expression's type after `is`",
            in_m!("var r = p is typeof(Cd);"),
            vec![ty("Cd")],
            k_m(),
        ),
        (
            "a `sizeof` expression's type in a switch case",
            in_m!("switch (p) { case sizeof(Ce): break; }"),
            vec![ty("Ce")],
            k_m(),
        ),
        (
            "an identifier in a binary expression in a switch case, a value",
            in_m!("switch (p) { case Cf + 1: break; }"),
            vec![],
            k_m(),
        ),
        (
            "an identifier in a prefix expression in a switch case, a value",
            in_m!("switch (p) { case -Ch: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a relational pattern's identifier after `is`, a value",
            in_m!("var r = p is > Max;"),
            vec![],
            k_m(),
        ),
        (
            "a relational pattern's identifier in a switch case",
            in_m!("switch (p) { case > Max: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a relational pattern's identifier in a switch case with `when`",
            in_m!("switch (p) { case > Max when ok: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a relational pattern's identifier in a switch arm",
            in_m!("var r = p switch { >= Max => 1, _ => 0 };"),
            vec![],
            k_m(),
        ),
        (
            "two relational patterns' identifiers under `and`",
            in_m!("var r = p is > Min and < Max;"),
            vec![],
            k_m(),
        ),
        (
            "a relational pattern's qualified name, the field it names",
            in_m!("var r = p is > Ns.Max;"),
            vec![field("Ns.Max", "Max")],
            k_m(),
        ),
        (
            "a call as a relational pattern's operand",
            in_m!("var r = p is > Pt(0, 0);"),
            vec![],
            k_m(),
        ),
        (
            "a generic declaration pattern after `is`",
            in_m!("var r = p is List<Db> l;"),
            vec![ty("Db"), ty("List")],
            k_m(),
        ),
        (
            "a qualified declaration pattern after `is`",
            in_m!("var r = p is Ns.Dc d;"),
            vec![],
            k_m(),
        ),
        (
            "a declaration pattern in a switch case",
            in_m!("switch (p) { case Dd d: break; }"),
            vec![ty("Dd")],
            k_m(),
        ),
        (
            "a declaration pattern in a switch case with `when`",
            in_m!("switch (p) { case Dl d when ok: break; }"),
            vec![ty("Dl")],
            k_m(),
        ),
        (
            "a declaration pattern in a switch arm",
            in_m!("var r = p switch { De d => 1, _ => 0 };"),
            vec![ty("De")],
            k_m(),
        ),
        (
            "a declaration pattern in a property subpattern",
            in_m!("var r = p is { P: Df d };"),
            vec![ty("Df")],
            k_m(),
        ),
        (
            "a declaration pattern in a list",
            in_m!("var r = p is [Dg d, ..];"),
            vec![ty("Dg")],
            k_m(),
        ),
        (
            "a declaration pattern in parentheses",
            in_m!("var r = p is (Di d);"),
            vec![ty("Di")],
            k_m(),
        ),
        (
            "a declaration pattern with a discard",
            in_m!("var r = p is Dj _;"),
            vec![ty("Dj")],
            k_m(),
        ),
        (
            "a declaration pattern with a parenthesized designation, an invocation to the grammar",
            in_m!("var r = p is Dk (a, b);"),
            vec![ty("Dk")],
            k_m(),
        ),
        (
            "a property pattern's type after `is`",
            in_m!("var r = p is Ra { P: 1 };"),
            vec![ty("Ra")],
            k_m(),
        ),
        (
            "an empty property pattern's type",
            in_m!("var r = p is Rb { };"),
            vec![ty("Rb")],
            k_m(),
        ),
        (
            "a property pattern's type with a designation",
            in_m!("var r = p is Rc { } rc;"),
            vec![ty("Rc")],
            k_m(),
        ),
        (
            "a generic property pattern's type",
            in_m!("var r = p is List<Rd> { Count: 1 };"),
            vec![ty("Rd"), ty("List")],
            k_m(),
        ),
        (
            "a qualified property pattern's type",
            in_m!("var r = p is Ns.Re { P: 1 };"),
            vec![],
            k_m(),
        ),
        (
            "a property pattern's type in a switch case",
            in_m!("switch (p) { case Rf { P: 1 }: break; }"),
            vec![ty("Rf")],
            k_m(),
        ),
        (
            "a property pattern's type in a switch arm",
            in_m!("var r = p switch { Rg { P: 1 } => 1, _ => 0 };"),
            vec![ty("Rg")],
            k_m(),
        ),
        (
            "a positional pattern with a property clause",
            in_m!("var r = p is Rh(1, 2) { P: 1 };"),
            vec![ty("Rh")],
            k_m(),
        ),
        (
            "an empty positional pattern after `is`",
            in_m!("var r = p is Ri();"),
            vec![ty("Ri")],
            k_m(),
        ),
        (
            "an empty positional pattern in a switch case",
            in_m!("switch (p) { case Rj(): break; }"),
            vec![ty("Rj")],
            k_m(),
        ),
        (
            "a declaring positional pattern with a designation in a switch case",
            in_m!("switch (p) { case Pu(var x, 0) d: break; }"),
            vec![ty("Pu")],
            k_m(),
        ),
        (
            "a `var` pattern after `is`",
            in_m!("var r = p is var v;"),
            vec![],
            k_m(),
        ),
        (
            "a `var` pattern in a switch case",
            in_m!("switch (p) { case var v: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a `var` tuple pattern after `is`",
            in_m!("var r = p is var (a, b);"),
            vec![],
            k_m(),
        ),
        (
            "a discard in a switch case",
            in_m!("switch (p) { case _: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a discard under `not`",
            in_m!("var r = p is not _;"),
            vec![],
            k_m(),
        ),
        (
            "a discard in a switch arm",
            in_m!("var r = p switch { _ => 0 };"),
            vec![],
            k_m(),
        ),
        (
            "a list pattern with a designation",
            in_m!("var r = p is [Lc] l;"),
            vec![ty("Lc")],
            k_m(),
        ),
        (
            "an empty list pattern",
            in_m!("var r = p is [];"),
            vec![],
            k_m(),
        ),
        (
            "a list pattern in a switch case",
            in_m!("switch (p) { case [Le, ..]: break; }"),
            vec![ty("Le")],
            k_m(),
        ),
        (
            "a list pattern in a switch arm",
            in_m!("var r = p switch { [Lf] => 1, _ => 0 };"),
            vec![ty("Lf")],
            k_m(),
        ),
        (
            "a slice before an element, a parse error to the grammar",
            in_m!("var r = p is [.., Lb];"),
            vec![],
            k_m(),
        ),
        (
            "a slice holding a `var` pattern, a parse error",
            in_m!("var r = p is [La, .. var rest];"),
            vec![],
            k_m(),
        ),
        (
            "a slice holding a declaration pattern, a parse error",
            in_m!("var r = p is [.. Ld d];"),
            vec![],
            k_m(),
        ),
        (
            "a generic name in a list case label",
            in_m!("switch (p) { case [Lp<int>]: break; }"),
            vec![ty("Lp")],
            k_m(),
        ),
        (
            "a member access in a list case label",
            in_m!("switch (p) { case [Lq.Lr]: break; }"),
            vec![field("Lq.Lr", "Lr")],
            k_m(),
        ),
        (
            "a positional pattern of constants in a list case label",
            in_m!("switch (p) { case [Le(0, 0)]: break; }"),
            vec![ty("Le")],
            k_m(),
        ),
        (
            "a `var` pattern in a list case label",
            in_m!("switch (p) { case [var x]: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a declaring positional pattern in a list case label with `when`, a parse error",
            in_m!("switch (p) { case [Ld(var x, 0)] when ok: break; }"),
            vec![ty("Ld")],
            k_m(),
        ),
        (
            "a declaring positional pattern in a list under `not` in a switch case",
            in_m!("switch (p) { case not [Lx(var x, 0)]: break; }"),
            vec![ty("Lx")],
            k_m(),
        ),
        (
            "a declaring positional pattern in a list in a case's property subpattern",
            in_m!("switch (p) { case { P: [Lz(var x, 0)] }: break; }"),
            vec![ty("Lz")],
            k_m(),
        ),
        (
            "a declaring positional pattern in parentheses in a list case label",
            in_m!("switch (p) { case [(Lw(var x, 0))]: break; }"),
            vec![ty("Lw")],
            k_m(),
        ),
        (
            "two lists under `or` in a switch case, each holding a declaring positional pattern",
            in_m!("switch (p) { case [Lo(var x, 0)] or [Lp(var y, 0)]: break; }"),
            vec![ty("Lo"), ty("Lp")],
            k_m(),
        ),
        (
            "a list case label with a designation holding a declaring positional pattern",
            in_m!("switch (p) { case [Lo(var x, 0)] l: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a positional pattern whose first argument is `var (a, b)`",
            in_m!("switch (p) { case [Lo(var (a, b), 0)]: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a `not` pattern and a declaring positional pattern",
            in_m!("switch (p) { case [not Ln, Lo(var x, 0)]: break; }"),
            vec![ty("Ln"), ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a `var` pattern and a declaring positional pattern",
            in_m!("switch (p) { case [var v, Lo(var x, 0)]: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "a declaring positional pattern in a list in a switch arm",
            in_m!("var r = p switch { [Ma(var x, 0)] => 1, _ => 0 };"),
            vec![ty("Ma")],
            k_m(),
        ),
        (
            "a declaring positional pattern in a list after `is`",
            in_m!("var r = p is [Mc(var x, 0)];"),
            vec![ty("Mc")],
            k_m(),
        ),
        (
            "a positional pattern of constants in a list after `is`",
            in_m!("var r = p is [Pl(0, 0), ..];"),
            vec![ty("Pl")],
            k_m(),
        ),
        (
            "a property subpattern's name",
            in_m!("var r = p is { Sa: 1 };"),
            vec![],
            k_m(),
        ),
        (
            "an extended property subpattern's name, the field it names",
            in_m!("var r = p is { Sb.Sc: 1 };"),
            vec![field("Sb.Sc", "Sc")],
            k_m(),
        ),
        (
            "an extended property subpattern's name on a lower-case object",
            in_m!("var r = p is { sd.Se: 1 };"),
            vec![field("Se", "Se")],
            k_m(),
        ),
        (
            "a positional pattern's named subpatterns",
            in_m!("var r = p is Pt(Sf: 1, Sg: 2) d;"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a tuple pattern's named subpatterns",
            in_m!("var r = p is (Sh: 1, Si: 2);"),
            vec![],
            k_m(),
        ),
        (
            "a positional pattern of constants in a property subpattern",
            in_m!("var r = p is { P: Pv(0, 0) };"),
            vec![ty("Pv")],
            k_m(),
        ),
        (
            "a declaring positional pattern in parentheses in a switch case",
            in_m!("switch (p) { case (Pa(var x, 0)): break; }"),
            vec![ty("Pa")],
            k_m(),
        ),
        (
            "a declaring positional pattern under `not` in a switch case",
            in_m!("switch (p) { case not Pb(var x, 0): break; }"),
            vec![ty("Pb")],
            k_m(),
        ),
        (
            "two declaring positional patterns under `or` in a switch case",
            in_m!("switch (p) { case Pc(var x, 0) or Pd(var y, 0): break; }"),
            vec![ty("Pc"), ty("Pd")],
            k_m(),
        ),
        (
            "two declaring positional patterns under `and` in a switch case",
            in_m!("switch (p) { case Pe(var x, 0) and Pf(var y, 0): break; }"),
            vec![ty("Pe"), ty("Pf")],
            k_m(),
        ),
        (
            "a declaring positional pattern in parentheses under `not` in a switch case",
            in_m!("switch (p) { case not (Pg(var x, 0)): break; }"),
            vec![ty("Pg")],
            k_m(),
        ),
        (
            "a declaring positional pattern in parentheses in a switch case with `when`",
            in_m!("switch (p) { case (Ps(var x, 0)) when ok: break; }"),
            vec![ty("Ps")],
            k_m(),
        ),
        (
            "a positional pattern of constants in parentheses in a switch case",
            in_m!("switch (p) { case (Pr(0, 0)): break; }"),
            vec![ty("Pr")],
            k_m(),
        ),
        (
            "a declaring positional pattern in a case's property subpattern",
            in_m!("switch (p) { case { P: Pq(var x, 0) }: break; }"),
            vec![ty("Pq")],
            k_m(),
        ),
        (
            "a declaring positional pattern in parentheses in a switch arm",
            in_m!("var r = p switch { (Ph(var x, 0)) => 1, _ => 0 };"),
            vec![ty("Ph")],
            k_m(),
        ),
        (
            "a declaring positional pattern under `not` in a switch arm",
            in_m!("var r = p switch { not Pi(var x, 0) => 1, _ => 0 };"),
            vec![ty("Pi")],
            k_m(),
        ),
        (
            "two declaring positional patterns under `or` in a switch arm",
            in_m!("var r = p switch { Pj(var x, 0) or Pk(var y, 0) => 1, _ => 0 };"),
            vec![ty("Pj"), ty("Pk")],
            k_m(),
        ),
        (
            "a declaring positional pattern in a switch arm with `when`",
            in_m!("var r = p switch { Pw(var x, 0) when ok => 1, _ => 0 };"),
            vec![ty("Pw")],
            k_m(),
        ),
        (
            "a declaring positional pattern in parentheses in a property subpattern",
            in_m!("var r = p is { P: (Pm(var x, 0)) };"),
            vec![ty("Pm")],
            k_m(),
        ),
        (
            "a declaring positional pattern in parentheses under `not` after `is`",
            in_m!("var r = p is not (Pn(var x, 0));"),
            vec![ty("Pn")],
            k_m(),
        ),
        (
            "a declaring positional pattern after `is`, a call of the `is` expression to the grammar",
            in_m!("var r = p is Pt(var x, 0);"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a positional pattern whose later argument declares after `is`",
            in_m!("var r = p is Pt(0, var y);"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a positional pattern whose first argument is typed after `is`",
            in_m!("var r = p is Pt(int u, 0);"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a positional pattern whose first argument discards after `is`",
            in_m!("var r = p is Pt(_, 0);"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a positional pattern whose first argument is `var (a, b)` after `is`, a recursive pattern",
            in_m!("var r = p is Pt(var (a, b), 0);"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a declaring positional pattern under `not` after `is`",
            in_m!("var r = p is not Pb(var x, 0);"),
            vec![ty("Pb")],
            k_m(),
        ),
        (
            "two positional patterns of constants under `and` after `is`",
            in_m!("var r = p is Pt(0, 0) and Qt(0, 0);"),
            vec![ty("Pt"), ty("Qt")],
            k_m(),
        ),
        (
            "two positional patterns of constants under `or` after `is`",
            in_m!("var r = p is Pt(0, 0) or Qt(0, 0);"),
            vec![ty("Pt"), ty("Qt")],
            k_m(),
        ),
        (
            "two declaring positional patterns under `or` after `is`",
            in_m!("var r = p is Pt(var x, 0) or Qt(var y, 0);"),
            vec![ty("Pt"), ty("Qt")],
            k_m(),
        ),
        (
            "a declaring positional pattern in a property subpattern after `is`",
            in_m!("var r = p is { P: Pq(var x, 0) };"),
            vec![ty("Pq")],
            k_m(),
        ),
        (
            "a positional pattern whose later argument declares in a list after `is`",
            in_m!("var r = p is [Mc(0, var y)];"),
            vec![ty("Mc")],
            k_m(),
        ),
        (
            "a positional pattern whose later argument declares in a switch case",
            in_m!("switch (p) { case Pt(0, var y): break; }"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a positional pattern of constants in a switch case with `when`",
            in_m!("switch (p) { case Pt(0, 0) when ok: break; }"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a declaring positional pattern in a switch case with `when`, a recursive pattern",
            in_m!("switch (p) { case Pt(var x, 0) when ok: break; }"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a positional pattern whose first argument is `var (a, b)` in a switch case, a recursive pattern",
            in_m!("switch (p) { case Pt(var (a, b), 0): break; }"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "stacked case labels each holding a declaring positional pattern, the second with `when`",
            in_m!("switch (p) { case Pt(var x, 0): case Qt(var y, 0) when ok: break; }"),
            vec![ty("Pt"), ty("Qt")],
            k_m(),
        ),
        (
            "a `var` tuple pattern in a switch case",
            in_m!("switch (p) { case var (a, b): break; }"),
            vec![],
            k_m(),
        ),
        (
            "a declaring positional pattern in a switch arm",
            in_m!("var r = p switch { Pt(var x, 0) => 1, _ => 0 };"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a positional pattern whose later argument declares in a switch arm",
            in_m!("var r = p switch { Pt(0, var y) => 1, _ => 0 };"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a list case label holding a positional pattern whose later argument declares",
            in_m!("switch (p) { case [Lo(0, var y)]: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "stacked list case labels each holding a declaring positional pattern, the second with `when`",
            in_m!("switch (p) { case [Lo(var x, 0)]: case [Lq(var y, 0)] when ok: break; }"),
            vec![ty("Lo"), ty("Lq")],
            k_m(),
        ),
        (
            "`nameof` as a declaration pattern's type in a switch case",
            in_m!("switch (s) { case nameof x: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a list case label with no `when` holding a declaring positional pattern, a collection expression in the switch section to the grammar",
            in_m!("switch (p) { case [La(var x, 0)]: break; }"),
            vec![ty("La")],
            k_m(),
        ),
        (
            "a list case label holding a declaring positional pattern and a slice",
            in_m!("switch (p) { case [Lb(var x, 0), ..]: break; }"),
            vec![ty("Lb")],
            k_m(),
        ),
        (
            "a list case label holding a slice and a declaring positional pattern",
            in_m!("switch (p) { case [.., Lo(var x, 0)]: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a constant and a declaring positional pattern",
            in_m!("switch (p) { case [0, Lc(var x, 0)]: break; }"),
            vec![ty("Lc")],
            k_m(),
        ),
        (
            "a list case label holding an identifier and a declaring positional pattern",
            in_m!("switch (p) { case [Ln, Lo(var x, 0)]: break; }"),
            vec![ty("Ln"), ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a generic name and a declaring positional pattern",
            in_m!("switch (p) { case [Lp<int>, Lo(var x, 0)]: break; }"),
            vec![ty("Lp"), ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding an identifier in parentheses and a declaring positional pattern",
            in_m!("switch (p) { case [(Lw), Lo(var x, 0)]: break; }"),
            vec![ty("Lw"), ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding two declaring positional patterns, one in parentheses",
            in_m!("switch (p) { case [(Lw(var y, 0)), Lo(var x, 0)]: break; }"),
            vec![ty("Lw"), ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a generic name in parentheses and a declaring positional pattern",
            in_m!("switch (p) { case [(Lp<int>), Lo(var x, 0)]: break; }"),
            vec![ty("Lp"), ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a member access and a declaring positional pattern",
            in_m!("switch (p) { case [Lq.Lr, Lo(var x, 0)]: break; }"),
            vec![field("Lq.Lr", "Lr"), ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a cast and a declaring positional pattern",
            in_m!("switch (p) { case [(Ca)0, Lo(var x, 0)]: break; }"),
            vec![ty("Ca"), ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding two declaring positional patterns",
            in_m!("switch (p) { case [Lk(var x, 0), Ll(var y, 0)]: break; }"),
            vec![ty("Lk"), ty("Ll")],
            k_m(),
        ),
        (
            "stacked list case labels each holding a declaring positional pattern",
            in_m!("switch (p) { case [Lf(var x, 0)]: case [Lg(var y, 0)]: break; }"),
            vec![ty("Lf"), ty("Lg")],
            k_m(),
        ),
        (
            "a list case label holding a positional pattern whose first argument discards",
            in_m!("switch (p) { case [Lo(_, 0)]: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "a list case label holding a positional pattern whose first argument is typed",
            in_m!("switch (p) { case [Lo(int u, 0)]: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "a list nested in a list case label's element, a nested collection expression to the grammar",
            in_m!("switch (p) { case [[Lm(var x, 0)]]: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a list in one pair of parentheses after `is` holding a declaring positional pattern, a collection expression in a parenthesized constant pattern to the grammar",
            in_m!("var r = p is ([Ly(var x, 0)]);"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses after `is` holding an identifier",
            in_m!("var r = p is ([Ly]);"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses after `is` holding a generic name",
            in_m!("var r = p is ([Lp<int>]);"),
            vec![ty("Lp")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses after `is` holding an identifier in parentheses and a positional pattern of constants",
            in_m!("var r = p is ([(Lw), Lo(0, 0)]);"),
            vec![ty("Lw"), ty("Lo")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses after `is` holding a generic name in parentheses",
            in_m!("var r = p is ([(Lp<int>)]);"),
            vec![ty("Lp")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses after `is` holding a positional pattern in parentheses",
            in_m!("var r = p is ([(Lw(0, 0))]);"),
            vec![ty("Lw")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses in a switch case holding a declaring positional pattern",
            in_m!("switch (p) { case ([Ly(var x, 0)]): break; }"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses in a switch case holding a positional pattern of constants",
            in_m!("switch (p) { case ([Ly(0, 0)]): break; }"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses in a switch case holding an identifier",
            in_m!("switch (p) { case ([Ly]): break; }"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses in a switch case with `when`",
            in_m!("switch (p) { case ([Ly(var x, 0)]) when ok: break; }"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses under `not` in a switch case",
            in_m!("switch (p) { case not ([Ly(var x, 0)]): break; }"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses in a switch arm",
            in_m!("var r = p switch { ([Ly(var x, 0)]) => 1, _ => 0 };"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in one pair of parentheses in a property subpattern",
            in_m!("var r = p is { P: ([Ly(var x, 0)]) };"),
            vec![ty("Ly")],
            k_m(),
        ),
        (
            "a list in two pairs of parentheses",
            in_m!("var r = p is (([Ly]));"),
            vec![],
            k_m(),
        ),
        (
            "a list nested in a parenthesized list's element",
            in_m!("var r = p is ([[Lm]]);"),
            vec![],
            k_m(),
        ),
        (
            "`nameof` in a list in one pair of parentheses",
            in_m!("var r = s is ([nameof(Foo)]);"),
            vec![],
            k_m(),
        ),
        (
            "`nameof` in parentheses in a switch case",
            in_m!("switch (s) { case (nameof(Foo)): break; }"),
            vec![],
            k_m(),
        ),
        (
            "a declaring `nameof(var x)` in parentheses in a switch case",
            in_m!("switch (s) { case (nameof(var x)): break; }"),
            vec![],
            k_m(),
        ),
        (
            "a declaring `nameof(var x)` under `not` in a switch case",
            in_m!("switch (s) { case not nameof(var x): break; }"),
            vec![],
            k_m(),
        ),
        (
            "`nameof` in parentheses in a switch arm",
            in_m!("var r = s switch { (nameof(Foo)) => 1, _ => 0 };"),
            vec![],
            k_m(),
        ),
        (
            "a declaring `nameof(var x)` in a list after `is`",
            in_m!("var r = s is [nameof(var x)];"),
            vec![],
            k_m(),
        ),
        (
            "a declaring `nameof(var x)` in a property subpattern",
            in_m!("var r = s is { Length: nameof(var x) };"),
            vec![],
            k_m(),
        ),
        (
            "`nameof` in a list case label",
            in_m!("switch (s) { case [nameof(Foo)]: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a declaring `nameof(var x)` in a list case label",
            in_m!("switch (s) { case [nameof(var x)]: break; }"),
            vec![],
            k_m(),
        ),
        (
            "`nameof` beside a declaring positional pattern in a list case label",
            in_m!("switch (p) { case [nameof(Foo), Lo(var x, 0)]: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "the identifier `nameof` beside a declaring positional pattern in a list case label",
            in_m!("switch (p) { case [nameof, Lo(var x, 0)]: break; }"),
            vec![ty("Lo")],
            k_m(),
        ),
        (
            "a positional pattern's argument subpattern",
            in_m!("var r = p is Pt(Inner(0, 0), 1);"),
            vec![ty("Pt")],
            k_m(),
        ),
        (
            "a positional pattern's argument subpattern with a designation",
            in_m!("var r = p is Pt(Inner(0, 0), 1) d;"),
            vec![ty("Pt"), ty("Inner")],
            k_m(),
        ),
        (
            "a tuple pattern's positional subpattern",
            in_m!("var r = (p, s) is (Pv(0, 0), _);"),
            vec![],
            k_m(),
        ),
        (
            "a tuple pattern's identifiers",
            in_m!("var r = (p, s) is (Ci, Cj);"),
            vec![],
            k_m(),
        ),
        (
            "a qualified positional pattern after `is`, a call to the grammar",
            in_m!("var r = p is Ns.Pw(0, 0);"),
            vec![call("Ns.Pw", "Pw")],
            k_m(),
        ),
        (
            "a qualified declaring positional pattern in a switch case",
            in_m!("switch (p) { case Ns.Pz(var x, 0): break; }"),
            vec![call("Ns.Pz", "Pz")],
            k_m(),
        ),
        (
            "a qualified declaring positional pattern in a list case label",
            in_m!("switch (p) { case [Ns.Lo(var x, 0)]: break; }"),
            vec![call("Ns.Lo", "Lo")],
            k_m(),
        ),
        (
            "a generic positional pattern after `is`, comparisons to the grammar",
            in_m!("var r = p is Px<A>(0, 0);"),
            vec![],
            k_m(),
        ),
        (
            "a generic declaring positional pattern in a switch case",
            in_m!("switch (p) { case Qa<A>(var x, 0): break; }"),
            vec![],
            k_m(),
        ),
        (
            "a generic declaring positional pattern in a list case label",
            in_m!("switch (p) { case [Lo<A>(var x, 0)]: break; }"),
            vec![],
            k_m(),
        ),
        (
            "a declaring positional pattern's argument subpattern in a list case label",
            in_m!("switch (p) { case [Lu(Lv(var x), 0)]: break; }"),
            vec![ty("Lu")],
            k_m(),
        ),
        (
            "a collection in a case's body",
            in_m!("switch (p) { case 0: Use([Mf(1)]); break; }"),
            vec![],
            k_m(),
        ),
        (
            "a collection in a case's `when`",
            in_m!("switch (p) { case int i when Has([Mg(i)]): break; }"),
            vec![],
            k_m(),
        ),
        (
            "a collection in an expression",
            in_m!("int[] r = [Mh(1), 2];"),
            vec![],
            k_m(),
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
