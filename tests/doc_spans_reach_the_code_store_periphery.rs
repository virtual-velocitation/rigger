//! Periphery (integration) tests for spec 117, RUST DOC SPANS ARE EXTRACTED and the docstring
//! spans of PYTHON STAGES ITS ROW, across the seam the crate's own tests stop short of: the real
//! `rigger graph build` extracts each file, the facts layer copies the file's and each
//! definition's doc span into the file's facts, and the code store persists those facts, so a
//! reader of the built store finds the spans extraction gave. The crate's tests pin the
//! extraction and the copy in process; these pin that the binary's own build stores them, and
//! that a rebuild replaces a rewritten file's spans and keeps an unchanged file's.
//!
//! NOT OWNED here: a fixture query's `@doc` capture (only a crate test can hand extraction a query
//! of its own; the Go and Python files below are that capture's reach through a bundled query),
//! the spans' serialized form (the symbols model's own test) and the readers that print a span
//! (spec 117's reader criteria).
//!
//! Every test extracts real source, so the file runs on the `symbols` lane alone.

use crate::common;

use common::cli::{rebuilt_graph, temp_project, BuiltGraph};
use rigger_domain::code_view::CodeView;

/// One file of a build: its root-relative path, its source, the file's doc span, and each
/// definition's local path with its doc span, in the order the store holds them. A span is the
/// 1-based first and last line, `None` when there is none.
type Row = (
    &'static str,
    &'static str,
    Option<(u32, u32)>,
    &'static [(&'static str, Option<(u32, u32)>)],
);

/// A file as a built store holds it: its path, its doc span and each definition's local path
/// with its doc span.
type Held = (
    String,
    Option<(u32, u32)>,
    Vec<(String, Option<(u32, u32)>)>,
);

/// The first build: a Rust file opening with two inner doc lines and holding a definition whose
/// outer doc lines have a `#[doc(hidden)]` between them beside an undocumented one, a Rust file
/// with no doc line, a Go file whose bundled query captures a three-line comment block over
/// one function and none over the next, and a Python file opening with a module docstring of two
/// parts in parentheses over two lines and holding a class, past its base class, whose body opens
/// with a four-line docstring beside an undocumented function.
const FIRST: [Row; 4] = [
    (
        "src/lib.rs",
        "//! The crate.\n//! It holds two functions.\n\n/// First.\n#[doc(hidden)]\n/// Last.\n\
         pub fn documented() {}\n\npub fn plain() {}\n",
        Some((1, 2)),
        &[("documented", Some((4, 6))), ("plain", None)],
    ),
    ("src/bare.rs", "pub fn bare() {}\n", None, &[("bare", None)]),
    (
        "p.go",
        "package p\n\n// Reads reads the input.\n// It returns nothing.\n// It never fails.\n\
         func Reads() {}\n\nfunc Bare() {}\n",
        None,
        &[("Reads", Some((3, 5))), ("Bare", None)],
    ),
    (
        "p.py",
        "(\"The \"\n \"shapes.\")\n\n\nclass Shape(Base):\n    \"\"\"A shape.\n\n    Of any size.\n    \
         \"\"\"\n\n\ndef bare():\n    pass\n",
        Some((1, 2)),
        &[("Shape", Some((6, 9))), ("bare", None)],
    ),
];

/// The rebuild: the first Rust file rewritten so its file span and `documented`'s are gone and
/// `plain` gains one, the second given a file span and a definition span, the Go file unchanged,
/// and the Python file rewritten so its string follows an import and is no module docstring,
/// `Shape`'s body opens with a bytes literal, which is no docstring, and `bare`'s, past its
/// parameter's annotation, opens with a docstring joined from two parts.
const NEXT: [Row; 4] = [
    (
        "src/lib.rs",
        "/// Now first.\npub fn plain() {}\n\npub fn documented() {}\n",
        None,
        &[("plain", Some((1, 1))), ("documented", None)],
    ),
    (
        "src/bare.rs",
        "//! Bare is documented now.\n\n/// B.\npub fn bare() {}\n",
        Some((1, 1)),
        &[("bare", Some((3, 3)))],
    ),
    FIRST[2],
    (
        "p.py",
        "import os\n\"\"\"Not the file's.\"\"\"\n\n\nclass Shape:\n    b\"Not a docstring.\"\n\n\n\
         def bare(s: Shape):\n    \"Now \" 'documented.'\n",
        None,
        &[("Shape", None), ("bare", Some((10, 10)))],
    ),
];

/// Each file `rows` names as `graph`'s code store holds it; a file the store lacks panics with
/// its path.
fn held(graph: &BuiltGraph, rows: &[Row]) -> Vec<Held> {
    rows.iter()
        .map(|&(path, ..)| {
            let facts = graph
                .view
                .file(path)
                .unwrap_or_else(|| panic!("the code store holds no {path}"));
            let defs = facts
                .defs
                .iter()
                .map(|d| (d.local.clone(), d.doc))
                .collect();
            (path.to_string(), facts.doc, defs)
        })
        .collect()
}

/// What `rows` say the code store holds, in [`held`]'s shape.
fn expected(rows: &[Row]) -> Vec<Held> {
    rows.iter()
        .map(|&(path, _, doc, defs)| {
            let defs = defs
                .iter()
                .map(|&(local, span)| (local.to_string(), span))
                .collect();
            (path.to_string(), doc, defs)
        })
        .collect()
}

/// GIVEN a project holding `FIRST`'s files, WHEN the operator runs `rigger graph build`, THEN
/// the code store holds each file's and each definition's doc span exactly; and WHEN the operator
/// rewrites the Rust and Python files to `NEXT`'s and builds again, THEN each rewritten file holds
/// its new spans, a span its source dropped gone, and the unchanged Go file keeps its own.
#[test]
fn a_build_stores_each_doc_span_and_a_rebuild_replaces_a_rewritten_files() {
    let project = temp_project();
    for (build, rows) in [("the first build", &FIRST), ("the rebuild", &NEXT)] {
        let files: Vec<(&str, &str)> = rows.iter().map(|&(path, text, ..)| (path, text)).collect();
        let graph = rebuilt_graph(project.path(), &files);
        assert_eq!(held(&graph, rows), expected(rows), "{build}");
    }
}
