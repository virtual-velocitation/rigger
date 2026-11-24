//! TYPESCRIPT AND JAVASCRIPT STAGE THEIR ROWS (spec 117, OTHER LANGUAGES): the facts the bundled
//! TypeScript, TSX and JavaScript grammars give a file hold a `type` mention for a type in type
//! position, an `impl` mention for each type an `implements` or `extends` clause names, a `const`
//! mention for a constant read, a `field` mention for `x.f` not called while a called `x.f()`
//! stays a call, a call named `Square.make` for `Square.make()`, and, as each one's doc span, the
//! `/** */` block directly above the TypeScript function `area`, the TSX class `View` and the
//! JavaScript class `Square`.

#![cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]

extern crate rigger_grounder as rigger;

#[path = "../../../tests/common/symbol_index.rs"]
mod symbol_index;

use rigger_domain::code_facts::Mention;
use rigger_domain::code_view::{Entry, Extract};
use rigger_domain::contextgraph::{REF_KIND_CONST, REF_KIND_FIELD, REF_KIND_IMPL, REF_KIND_TYPE};
use rigger_grounder::facts::FactsExtractor;
use symbol_index::mention_in;

/// A case: what it shows, the file's path, its source, the mentions its facts hold, whole and in
/// order, and each definition's leaf with its doc span, in order.
type Case = (
    &'static str,
    &'static str,
    &'static str,
    Vec<Mention>,
    Vec<(&'static str, Option<(u32, u32)>)>,
);

/// TYPESCRIPT AND JAVASCRIPT STAGE THEIR ROWS: in TypeScript, and in TSX alike, an interface in
/// an array type is a `type` mention, as is every type a type argument, a dotted name or a
/// generic names; each type an `implements` or an interface's `extends` names, dotted or
/// generic, is an `impl` mention, a generic one named without its type arguments, as is the class
/// a class extends; and a constant read is a `const` mention. In JavaScript the class
/// `extends Base` names is an `impl` mention, a dotted one qualified by its object; `x.f` not
/// called is a `field` mention, read, written, an inner chain, a subscript's object, an argument
/// or a private member alike, while a called `x.f()`, `new Ns.Box` and `new Ns.MAX()` are calls;
/// a member spelled as a constant is a `const` mention; and `Square.make()` is a call named
/// `Square.make` of the leaf `make`, each qualified by a capitalised object alone, a member chain
/// by its last name, so `Ns.Kit.make()` is the call `Kit.make`. In TypeScript `x.f<T>()` and
/// `Square.make<T>()` are calls of their leaf, unqualified, as `Square.make<T>()` is in TSX, and
/// the uncalled instantiation `x.g<T>` is a field. The `/** */` block directly above the
/// TypeScript function `area`, the TSX class `View` and the JavaScript class `Square` is each
/// one's doc span, over the production extractor.
#[test]
fn typescript_and_javascript_stage_their_rows() {
    let at =
        |enclosing, name, leaf, kind, line| mention_in(enclosing, name, leaf, kind, line, false);
    // A mention inside the file's first definition, named by its leaf.
    let in_first = |name, kind, line| at(Some(0), name, name, kind, line);
    let cases: [Case; 28] = [
        (
            "a TypeScript array of an interface",
            "shapes.ts",
            "function area(sides: IShape[]) {}\n",
            vec![in_first("IShape", Some(REF_KIND_TYPE), 1)],
            vec![("area", None)],
        ),
        (
            "a TypeScript implements clause",
            "shapes.ts",
            "class Square implements IShape {}\n",
            vec![in_first("IShape", Some(REF_KIND_IMPL), 1)],
            vec![("Square", None)],
        ),
        (
            "a TypeScript constant read",
            "shapes.ts",
            "function limit() {\n    return LIMIT;\n}\n",
            vec![in_first("LIMIT", Some(REF_KIND_CONST), 2)],
            vec![("limit", None)],
        ),
        (
            "a TypeScript `/** */` block",
            "shapes.ts",
            "/** The area. */\nfunction area() {}\n",
            vec![],
            vec![("area", Some((1, 1)))],
        ),
        (
            "a TypeScript interface's extends, a dotted and a generic implements",
            "shapes.ts",
            "interface Shape extends Base {}\nclass Square implements Ns.Shape, Box<Side> {}\n",
            vec![
                in_first("Base", Some(REF_KIND_IMPL), 1),
                at(Some(1), "Ns.Shape", "Shape", Some(REF_KIND_IMPL), 2),
                at(Some(1), "Side", "Side", Some(REF_KIND_TYPE), 2),
                at(Some(1), "Box", "Box", Some(REF_KIND_IMPL), 2),
            ],
            vec![("Shape", None), ("Square", None)],
        ),
        (
            "a TypeScript interface's dotted and generic extends",
            "shapes.ts",
            "interface Shape extends Ns.Base, Box<Side> {}\n",
            vec![
                at(Some(0), "Ns.Base", "Base", Some(REF_KIND_IMPL), 1),
                in_first("Side", Some(REF_KIND_TYPE), 1),
                in_first("Box", Some(REF_KIND_IMPL), 1),
            ],
            vec![("Shape", None)],
        ),
        (
            "a TypeScript class's extends",
            "shapes.ts",
            "class Square extends Base {}\n",
            vec![in_first("Base", Some(REF_KIND_IMPL), 1)],
            vec![("Square", None)],
        ),
        (
            "a TypeScript class's dotted extends",
            "shapes.ts",
            "class Square extends Ns.Base {}\n",
            vec![at(Some(0), "Ns.Base", "Base", Some(REF_KIND_IMPL), 1)],
            vec![("Square", None)],
        ),
        (
            "a TypeScript type argument, a dotted type and a generic",
            "shapes.ts",
            "function f(m: Map<K, Ns.V>) {}\n",
            vec![
                in_first("K", Some(REF_KIND_TYPE), 1),
                at(Some(0), "Ns.V", "V", Some(REF_KIND_TYPE), 1),
                in_first("Map", Some(REF_KIND_TYPE), 1),
            ],
            vec![("f", None)],
        ),
        (
            "a TypeScript generic member call",
            "shapes.ts",
            "function f(x) {\n    x.f<T>();\n    Square.make<T>();\n}\n",
            vec![
                in_first("f", None, 2),
                in_first("T", Some(REF_KIND_TYPE), 2),
                in_first("make", None, 3),
            ],
            vec![("f", None)],
        ),
        (
            "a TSX generic member call",
            "view.tsx",
            "function f() {\n    Square.make<T>();\n}\n",
            vec![
                in_first("make", None, 2),
                in_first("T", Some(REF_KIND_TYPE), 2),
            ],
            vec![("f", None)],
        ),
        (
            "a TypeScript instantiation not called",
            "shapes.ts",
            "function f(x) {\n    return x.g<T>;\n}\n",
            vec![
                in_first("g", Some(REF_KIND_FIELD), 2),
                in_first("T", Some(REF_KIND_TYPE), 2),
            ],
            vec![("f", None)],
        ),
        (
            "a TSX class implementing an interface, its array and a constant read",
            "view.tsx",
            "/** A view. */\nclass View implements IShape {\n    draw(sides: IShape[]) {\n        \
             return LIMIT;\n    }\n}\n",
            vec![
                in_first("IShape", Some(REF_KIND_IMPL), 2),
                at(Some(1), "IShape", "IShape", Some(REF_KIND_TYPE), 3),
                at(Some(1), "LIMIT", "LIMIT", Some(REF_KIND_CONST), 4),
            ],
            vec![("View", Some((1, 1))), ("draw", None)],
        ),
        (
            "a JavaScript extends clause",
            "shapes.js",
            "class Square extends Base {}\n",
            vec![in_first("Base", Some(REF_KIND_IMPL), 1)],
            vec![("Square", None)],
        ),
        (
            "a JavaScript member not called",
            "shapes.js",
            "function f(x) {\n    return x.f;\n}\n",
            vec![in_first("f", Some(REF_KIND_FIELD), 2)],
            vec![("f", None)],
        ),
        (
            "a JavaScript call of a capitalised object's member",
            "shapes.js",
            "function f() {\n    Square.make();\n}\n",
            vec![at(Some(0), "Square.make", "make", None, 2)],
            vec![("f", None)],
        ),
        (
            "a JavaScript call of a capitalised member chain's member",
            "shapes.js",
            "function f() {\n    Ns.Kit.make();\n}\n",
            vec![
                at(Some(0), "Ns.Kit", "Kit", Some(REF_KIND_FIELD), 2),
                at(Some(0), "Kit.make", "make", None, 2),
            ],
            vec![("f", None)],
        ),
        (
            "a capitalised member chain's member not called",
            "shapes.js",
            "function f() {\n    return Ns.Kit.size;\n}\n",
            vec![
                at(Some(0), "Ns.Kit", "Kit", Some(REF_KIND_FIELD), 2),
                at(Some(0), "Kit.size", "size", Some(REF_KIND_FIELD), 2),
            ],
            vec![("f", None)],
        ),
        (
            "a JavaScript `/** */` block",
            "shapes.js",
            "/** A square. */\nclass Square {}\n",
            vec![],
            vec![("Square", Some((1, 1)))],
        ),
        (
            "a JavaScript dotted extends",
            "shapes.js",
            "class Square extends Ns.Base {}\n",
            vec![at(Some(0), "Ns.Base", "Base", Some(REF_KIND_IMPL), 1)],
            vec![("Square", None)],
        ),
        (
            "a called member and a member in its arguments",
            "shapes.js",
            "function f(x, y) {\n    x.f(y.g);\n}\n",
            vec![
                in_first("f", None, 2),
                in_first("g", Some(REF_KIND_FIELD), 2),
            ],
            vec![("f", None)],
        ),
        (
            "a member written and one updated",
            "shapes.js",
            "function f(a, c) {\n    a.b = 1;\n    c.d += 1;\n}\n",
            vec![
                in_first("b", Some(REF_KIND_FIELD), 2),
                in_first("d", Some(REF_KIND_FIELD), 3),
            ],
            vec![("f", None)],
        ),
        (
            "an inner chain and a subscript's object",
            "shapes.js",
            "function f(x, a) {\n    return x.y.z + a.k[0];\n}\n",
            vec![
                in_first("y", Some(REF_KIND_FIELD), 2),
                in_first("z", Some(REF_KIND_FIELD), 2),
                in_first("k", Some(REF_KIND_FIELD), 2),
            ],
            vec![("f", None)],
        ),
        (
            "a `new` with no argument list",
            "shapes.js",
            "function f() {\n    new Ns.Box;\n}\n",
            vec![at(Some(0), "Ns.Box", "Box", None, 2)],
            vec![("f", None)],
        ),
        (
            "members spelled as constants",
            "shapes.js",
            "function h(cfg) {\n    Config.MAX;\n    cfg.TIMEOUT;\n    this.LIMIT;\n    \
             Ns.Kit.CAP;\n    Config.Max;\n}\n",
            vec![
                at(Some(0), "Config.MAX", "MAX", Some(REF_KIND_CONST), 2),
                in_first("TIMEOUT", Some(REF_KIND_CONST), 3),
                in_first("LIMIT", Some(REF_KIND_CONST), 4),
                at(Some(0), "Ns.Kit", "Kit", Some(REF_KIND_FIELD), 5),
                at(Some(0), "Kit.CAP", "CAP", Some(REF_KIND_CONST), 5),
                at(Some(0), "Config.Max", "Max", Some(REF_KIND_FIELD), 6),
            ],
            vec![("h", None)],
        ),
        (
            "a constant called, constructed or assigned",
            "shapes.js",
            "function h() {\n    Config.MAX();\n    new Ns.MAX();\n    FOO();\n    FOO = 3;\n}\n",
            vec![
                at(Some(0), "Config.MAX", "MAX", None, 2),
                at(Some(0), "Ns.MAX", "MAX", None, 3),
                in_first("FOO", None, 4),
            ],
            vec![("h", None)],
        ),
        (
            "a private member",
            "shapes.js",
            "class C {\n    m() {\n        return this.#p;\n    }\n}\n",
            vec![at(Some(1), "#p", "#p", Some(REF_KIND_FIELD), 3)],
            vec![("C", None), ("m", None)],
        ),
        (
            "a capitalised object's private member",
            "shapes.js",
            "class Box {\n    static m() {\n        return Box.#p;\n    }\n}\n",
            vec![at(Some(1), "Box.#p", "#p", Some(REF_KIND_FIELD), 3)],
            vec![("Box", None), ("m", None)],
        ),
    ];
    for (case, path, source, mentions, docs) in cases {
        let entry = Entry {
            path: path.to_string(),
            blob: String::new(),
        };
        let facts = FactsExtractor.extract(&entry, source.as_bytes());
        assert_eq!(facts.mentions, mentions, "{case}: the mentions");
        assert_eq!(
            facts
                .defs
                .iter()
                .map(|d| (d.leaf.as_str(), d.doc))
                .collect::<Vec<_>>(),
            docs,
            "{case}: each definition's doc span"
        );
    }
}
