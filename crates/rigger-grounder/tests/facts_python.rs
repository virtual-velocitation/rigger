//! PYTHON STAGES ITS ROW (spec 117, OTHER LANGUAGES): the facts the bundled Python grammar gives a
//! file hold a `const` mention for a constant read, a `type` mention for a capitalised name in an
//! annotation, an `impl` mention for a base class, a `field` mention for an attribute that is not
//! called while a called one stays a call, a call named `Shape.area` for `Shape.area()`, and a
//! docstring's doc span, a definition's and the module's, found as Python finds a docstring.

#![cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]

extern crate rigger_grounder as rigger;

#[path = "../../../tests/common/symbol_index.rs"]
mod symbol_index;

use rigger_domain::code_facts::Mention;
use rigger_domain::code_view::{Entry, Extract};
use rigger_domain::contextgraph::{REF_KIND_CONST, REF_KIND_FIELD, REF_KIND_IMPL, REF_KIND_TYPE};
use rigger_grounder::facts::FactsExtractor;
use symbol_index::mention_in;

/// A case: what it shows, the Python source, the mentions its facts hold, whole and in order, each
/// definition's leaf with its doc span, in order, and the file's doc span.
type Case = (
    &'static str,
    &'static str,
    Vec<Mention>,
    Vec<(&'static str, Option<(u32, u32)>)>,
    Option<(u32, u32)>,
);

/// PYTHON STAGES ITS ROW: a read of a module constant is a `const` mention, an all-caps attribute
/// of a lowercase object (`cfg.TIMEOUT`) included; a capitalised name in an annotation is a `type`
/// mention, an all-caps one included; each base class, dotted or subscripted, is an `impl` mention
/// and a keyword argument none; `x.f` read or assigned and not called is a `field` mention,
/// qualified by an object that is exactly a capitalised identifier, `Shape.side` and the member
/// constant `Config.MAX` alike, while a called `x.g()` is a call; `Shape.area()` is a call named
/// `Shape.area` of the leaf `area`; a call of an all-caps name, `LOG()`, `Shape.MAX()` and
/// `cfg.RUN()`, is a call as any other is; and a docstring is that definition's or the file's doc
/// span: the first statement of a body or of the module, comments before it aside, when it is a
/// str literal alone, joined from parts or in parentheses, and never a bytes, formatted or
/// template one, over the production extractor.
#[test]
fn python_stages_its_row() {
    // A mention inside the definition at index `at`, written `name` of `leaf`.
    let in_def = |at: u32, name: &str, leaf: &str, kind: Option<&str>, line: u32| {
        mention_in(Some(at), name, leaf, kind, line, false)
    };
    let cases: [Case; 20] = [
        (
            "a module constant's read",
            "LIMIT = 3\n\n\ndef size():\n    return LIMIT\n",
            vec![in_def(1, "LIMIT", "LIMIT", Some(REF_KIND_CONST), 5)],
            vec![("LIMIT", None), ("size", None)],
            None,
        ),
        (
            "a member constant's read",
            "def limits(cfg):\n    return Config.MAX + cfg.TIMEOUT\n",
            vec![
                in_def(0, "Config.MAX", "MAX", Some(REF_KIND_FIELD), 2),
                in_def(0, "TIMEOUT", "TIMEOUT", Some(REF_KIND_CONST), 2),
            ],
            vec![("limits", None)],
            None,
        ),
        (
            "an annotation",
            "def draw(s: Shape, n: int, k: ID) -> Area:\n    pass\n",
            vec![
                in_def(0, "Shape", "Shape", Some(REF_KIND_TYPE), 1),
                in_def(0, "ID", "ID", Some(REF_KIND_TYPE), 1),
                in_def(0, "Area", "Area", Some(REF_KIND_TYPE), 1),
            ],
            vec![("draw", None)],
            None,
        ),
        (
            "a base class",
            "class Square(Shape, abc.ABC, Generic[T], metaclass=Meta):\n    pass\n",
            vec![
                in_def(0, "Shape", "Shape", Some(REF_KIND_IMPL), 1),
                in_def(0, "ABC", "ABC", Some(REF_KIND_IMPL), 1),
                in_def(0, "Generic", "Generic", Some(REF_KIND_IMPL), 1),
                in_def(0, "T", "T", Some(REF_KIND_CONST), 1),
            ],
            vec![("Square", None)],
            None,
        ),
        (
            "an attribute not called and a called one",
            "def move(x):\n    x.f = Shape.side + x.g()\n",
            vec![
                in_def(0, "f", "f", Some(REF_KIND_FIELD), 2),
                in_def(0, "Shape.side", "side", Some(REF_KIND_FIELD), 2),
                in_def(0, "g", "g", None, 2),
            ],
            vec![("move", None)],
            None,
        ),
        (
            "a call on a capitalised object and on another",
            "def total(s):\n    return Shape.area() + s.area()\n",
            vec![
                in_def(0, "Shape.area", "area", None, 2),
                in_def(0, "area", "area", None, 2),
            ],
            vec![("total", None)],
            None,
        ),
        (
            "a call of an all-caps name, plain, on a capitalised object and on another",
            "def run(cfg):\n    LOG()\n    Shape.MAX()\n    cfg.RUN()\n",
            vec![
                in_def(0, "LOG", "LOG", None, 2),
                in_def(0, "Shape.MAX", "MAX", None, 3),
                in_def(0, "RUN", "RUN", None, 4),
            ],
            vec![("run", None)],
            None,
        ),
        (
            "a module, a class and a function docstring",
            "\"\"\"The shapes.\"\"\"\n\n\nclass Shape:\n    \"\"\"A shape.\"\"\"\n\n    def area(self):\n        \"\"\"Its area.\n\n        In units.\n        \"\"\"\n        return 0\n",
            vec![],
            vec![("Shape", Some((5, 5))), ("area", Some((8, 11)))],
            Some((1, 1)),
        ),
        (
            "a module docstring after a shebang and a comment",
            "#!/usr/bin/env python\n# A comment.\n\"\"\"The tool.\"\"\"\n\nimport os\n",
            vec![],
            vec![],
            Some((3, 3)),
        ),
        (
            "a docstring after a comment, and a prefixed one on the header's line",
            "class K:\n    # A comment.\n    \"\"\"K doc.\"\"\"\n\n\ndef one(): r\"\"\"Raw.\"\"\"\n",
            vec![],
            vec![("K", Some((3, 3))), ("one", Some((6, 6)))],
            None,
        ),
        (
            "a string that is not the first statement",
            "import os\n\"\"\"Not a docstring.\"\"\"\n\n\ndef late():\n    x = 1\n    \"\"\"Not its docstring.\"\"\"\n",
            vec![],
            vec![("late", None)],
            None,
        ),
        (
            "a joined, a continued and a parenthesised docstring, one over two lines past a comment",
            "def joined(s: Shape):\n    \"a\" 'b'\n\n\ndef continued():\n    \"a\" \\\n    r\"b\"\n\n\ndef wrapped():\n    (\"P.\")\n\n\ndef long():\n    (\"long \"  # it's \"x\"\n     \"doc\")\n",
            vec![in_def(0, "Shape", "Shape", Some(REF_KIND_TYPE), 1)],
            vec![
                ("joined", Some((2, 2))),
                ("continued", Some((6, 7))),
                ("wrapped", Some((11, 11))),
                ("long", Some((15, 16))),
            ],
            None,
        ),
        (
            "a bytes, a formatted, a mixed or a template string first is no docstring",
            "def raw_bytes(s: Shape):\n    rb\"x\"\n\n\ndef formatted():\n    f\"x\"\n\n\ndef mixed():\n    \"a\" Fr\"b\"\n\n\ndef template():\n    t\"x\"\n\n\nclass Lid(Box):\n    b\"x\"\n",
            vec![
                in_def(0, "Shape", "Shape", Some(REF_KIND_TYPE), 1),
                in_def(4, "Box", "Box", Some(REF_KIND_IMPL), 17),
            ],
            vec![
                ("raw_bytes", None),
                ("formatted", None),
                ("mixed", None),
                ("template", None),
                ("Lid", None),
            ],
            None,
        ),
        (
            "a module's parenthesised docstring and a class docstring past its base",
            "(\"The \"\n \"shapes.\")\n\n\nclass Lid(Box):\n    \"\"\"A lid.\"\"\"\n",
            vec![in_def(0, "Box", "Box", Some(REF_KIND_IMPL), 5)],
            vec![("Lid", Some((6, 6)))],
            Some((1, 2)),
        ),
        (
            "a module's joined docstring after a comment",
            "# A comment.\n\"The \" 'tool.'\n",
            vec![],
            vec![],
            Some((2, 2)),
        ),
        (
            "a module's bytes string first is no docstring",
            "b\"The tool.\"\n\nimport os\n",
            vec![],
            vec![],
            None,
        ),
        (
            "a tuple a str leads first is no docstring, in the module, a function and a class",
            "\"a\", \"b\"\n\n\ndef pair():\n    \"a\", \"b\"\n\n\ndef wrapped_pair():\n    (\"a\"), \"b\"\n\n\ndef joined_pair():\n    \"a\" \"b\", \"c\"\n\n\nclass Pair:\n    \"a\", \"b\"\n\n\nclass WrappedPair:\n    (\"a\"), \"b\"\n\n\nclass JoinedPair:\n    \"a\" \"b\", \"c\"\n",
            vec![],
            vec![
                ("pair", None),
                ("wrapped_pair", None),
                ("joined_pair", None),
                ("Pair", None),
                ("WrappedPair", None),
                ("JoinedPair", None),
            ],
            None,
        ),
        (
            "a module's tuple a parenthesised str leads first is no docstring",
            "(\"a\"), \"b\"\n",
            vec![],
            vec![],
            None,
        ),
        (
            "a module's tuple a joined str leads first is no docstring",
            "\"a\" \"b\", \"c\"\n",
            vec![],
            vec![],
            None,
        ),
        (
            "a docstring a comment follows on its line",
            "\"\"\"The tool.\"\"\"  # a comment\n\n\ndef noted():\n    \"\"\"Noted.\"\"\"  # a comment\n",
            vec![],
            vec![("noted", Some((5, 5)))],
            Some((1, 1)),
        ),
    ];
    for (case, source, mentions, docs, file_doc) in cases {
        let entry = Entry {
            path: "p.py".to_string(),
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
        assert_eq!(facts.doc, file_doc, "{case}: the file's doc span");
    }
}
