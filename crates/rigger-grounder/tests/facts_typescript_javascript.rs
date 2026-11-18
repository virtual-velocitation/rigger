#![cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]

use rigger_domain::code_view::{Entry, Extract};
use rigger_grounder::facts::FactsExtractor;

#[test]
fn probe() {
    let rows: &[(&str, &str)] = &[
        ("a.ts", "interface IShape {}\nclass Sq implements IShape {\n    sides: IShape[] = [];\n}\nconst LIMIT = 3;\nfunction f(): number {\n    return LIMIT;\n}\n/**\n * Doc.\n */\nfunction g() {}\n"),
        ("a.ts", "function f(x) {\n    x.f<T>();\n    Square.make<T>();\n    return x.g;\n}\n"),
        ("a.ts", "function f(a: Map<K, V>, b: A | B, c: A & B, d: [A, B]): Promise<R> { let e: Ns.T; }\n"),
        ("a.ts", "interface I extends J, K {}\nclass C extends D implements E, F<G> {}\nclass H extends Ns.D {}\n"),
        ("a.ts", "function id<T>(t: T): T { return t; }\ntype M<T> = { [K in keyof T]: T[K] };\ntype U<T> = T extends Array<infer E> ? E : never;\n"),
        ("a.ts", "import { A } from 'a';\nimport B from 'b';\nexport { C };\nconst D = B;\nfunction p(E) { return E; }\nlet F = 1;\nF = 2;\n"),
        ("a.tsx", "class Sq implements IShape {\n    sides: IShape[] = [];\n}\nconst x = <A.B />;\n"),
        ("a.js", "class Kid extends Base {}\nclass K2 extends Ns.Base {}\nfunction h(x) {\n    x.f;\n    x.g();\n    Square.make();\n    Ns.Kit.Box;\n    return Ns.Kit.make();\n}\n/**\n * Doc.\n */\nfunction g() {}\n"),
        ("a.js", "new Ns.Box();\nnew Ns.Kit.Box();\nnew Box();\nconst LIMIT = 3;\nconst A = B;\nf(LIMIT, Limit, limit);\nx.f(y.g);\nthis.p;\nsuper.q();\n"),
        ("a.js", "import { A } from 'a';\nexport const B = 1;\nconst { C } = o;\nfunction p(D = E) { return D; }\nobj.Kit.LIMIT;\n"),
    ];
    for (path, src) in rows {
        let facts = FactsExtractor.extract(
            &Entry {
                path: path.to_string(),
                blob: String::new(),
            },
            src.as_bytes(),
        );
        println!("=== {path}\n{src}");
        for d in &facts.defs {
            println!("DEF {} {} line={} doc={:?}", d.local, d.kind, d.line, d.doc);
        }
        for m in &facts.mentions {
            println!(
                "MEN enc={:?} name={} leaf={} kind={:?} line={}",
                m.enclosing, m.name, m.leaf, m.kind, m.line
            );
        }
    }
    panic!("probe");
}
