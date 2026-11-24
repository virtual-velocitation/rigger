//! Periphery (integration) tests for spec 117, RUST DOC SPANS ARE EXTRACTED, and the `/** */`
//! clause of TYPESCRIPT AND JAVASCRIPT STAGE THEIR ROWS, across the seam the crate's own tests stop
//! short of: the real `rigger graph build` extracts each file, the facts layer copies the file's
//! and each definition's doc span into the file's facts, and the code store persists those facts,
//! so a reader of the built store finds the spans extraction gave. The crate's tests pin the
//! extraction and the copy in process; these pin that the binary's own build stores them, and
//! that a rebuild replaces a rewritten file's spans and keeps an unchanged file's. And, for the
//! doc half of SHOW PRINTS THE DOC AND THE REFERRERS, across the seam `ContextGraph::doc`'s own tests stop
//! short of (they read fake lines through a fake extractor): the code store's head carries the
//! extractor the store opened with, and a node's doc composed over that head is the extractor's
//! text of its stored span read from the reader's own worktree.
//!
//! NOT OWNED here: a fixture query's `@doc` capture (only a crate test can hand extraction a query
//! of its own; the Go, TypeScript and JavaScript files below are that capture's reach through a
//! bundled query), the spans' serialized form (the symbols model's own test) and the `--show`
//! print of a doc (`tests/graph_honesty_label_cli.rs`).
//!
//! Every test extracts real source, so the file runs on the `symbols` lane alone.

use crate::common;

use common::cli::{build_head, rebuilt_graph, repository_code_store, temp_project, BuiltGraph};
use common::fixtures::{head_honesty, write_files};
use rigger_domain::code_view::{CodeView, Compared, OpenCodeView, Tests, ViewFilter};
use rigger_domain::composed::{Composed, DocReader, Indexed};
use rigger_domain::contextgraph::ContextGraph;
use rigger_store::code::WorktreeLines;
use std::path::Path;

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
/// with no doc line, a Go file whose bundled query captures a three-line comment block over one
/// function and none over the next, a TypeScript and a JavaScript file whose bundled queries
/// each capture a `/** */` block over one definition and none over the next, and a Rust file
/// whose one definition, a test, has an outer doc line.
const FIRST: [Row; 6] = [
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
        "f.ts",
        "/**\n * The area.\n * Of a shape.\n */\nfunction area() {}\n\nfunction bare() {}\n",
        None,
        &[("area", Some((1, 4))), ("bare", None)],
    ),
    (
        "f.js",
        "/**\n * A square.\n */\nclass Square {}\n\nclass Plain {}\n",
        None,
        &[("Square", Some((1, 3))), ("Plain", None)],
    ),
    (
        "src/checks.rs",
        "/// Checks.\n#[test]\nfn checks() {}\n",
        None,
        &[("checks", Some((1, 1)))],
    ),
];

/// The rebuild: the first Rust file rewritten so its file span and `documented`'s are gone and
/// `plain` gains one, the second given a file span and a definition span, the TypeScript file
/// rewritten so `area`'s span is gone and `bare` gains one, the Go, JavaScript and test files
/// unchanged.
const NEXT: [Row; 6] = [
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
        "f.ts",
        "function area() {}\n\n/** Now documented. */\nfunction bare() {}\n",
        None,
        &[("area", None), ("bare", Some((3, 3)))],
    ),
    FIRST[4],
    FIRST[5],
];

/// The doc text of `p.go`'s `Reads`, which both builds hold.
const READS_DOC: &str = "// Reads reads the input.\n// It returns nothing.\n// It never fails.";

/// The doc text of `f.js`'s `Square`, which both builds hold.
const SQUARE_DOC: &str = "/**\n* A square.\n*/";

/// Each node a reader asks the doc of, with the doc it reads in a worktree holding `FIRST`'s
/// files and in one holding `NEXT`'s: a Rust span's comment text (`//!` and `///` lines without
/// their markers, `#[doc(hidden)]` dropped), the trimmed lines of a span in a language with no doc
/// rule, and none for a file or definition with no span or a test definition.
const DOCS: &[(&str, [Option<&str>; 2])] = &[
    (
        "src/lib.rs",
        [Some("The crate.\nIt holds two functions."), None],
    ),
    ("src/lib.rs::documented", [Some("First.\nLast."), None]),
    ("src/lib.rs::plain", [None, Some("Now first.")]),
    ("src/bare.rs", [None, Some("Bare is documented now.")]),
    ("src/bare.rs::bare", [None, Some("B.")]),
    ("p.go::Reads", [Some(READS_DOC), Some(READS_DOC)]),
    ("p.go::Bare", [None, None]),
    (
        "f.ts::area",
        [Some("/**\n* The area.\n* Of a shape.\n*/"), None],
    ),
    ("f.ts::bare", [None, Some("/** Now documented. */")]),
    ("f.js::Square", [Some(SQUARE_DOC), Some(SQUARE_DOC)]),
    ("f.js::Plain", [None, None]),
    ("src/checks.rs::checks", [None, None]),
];

/// Each of `rows`' files as `(path, source)`.
fn sources(rows: &[Row]) -> Vec<(&'static str, &'static str)> {
    rows.iter().map(|&(path, text, ..)| (path, text)).collect()
}

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
/// rewrites the first two Rust files and the TypeScript file to `NEXT`'s and builds again, THEN
/// each rewritten file holds its new spans, a span its source dropped gone, and the unchanged Go,
/// JavaScript and test files keep their own.
#[test]
fn a_build_stores_each_doc_span_and_a_rebuild_replaces_a_rewritten_files() {
    let project = temp_project();
    for (build, rows) in [("the first build", &FIRST), ("the rebuild", &NEXT)] {
        let graph = rebuilt_graph(project.path(), &sources(rows));
        assert_eq!(held(&graph, rows), expected(rows), "{build}");
    }
}

/// GIVEN a project built from `FIRST`'s files, WHEN a reader in its worktree composes the code
/// store's head with a doc reader over that worktree and the extractor the head carries, and asks
/// each node of `DOCS` its doc, THEN the head matches the worktree and each node reads its `FIRST`
/// doc; and WHEN the worktree is rewritten to `NEXT`'s files, uncommitted, THEN the head overlays
/// the three rewritten files and each node reads its `NEXT` doc, from the worktree's text.
#[test]
fn a_node_s_doc_is_its_span_read_from_the_reader_s_worktree() {
    let project = temp_project();
    let root = project.path();
    write_files(root, &sources(&FIRST));
    build_head(root);
    assert_docs(root, "the built tree", 0, 0);
    write_files(root, &sources(&NEXT));
    assert_docs(root, "the rewritten worktree", 3, 1);
}

/// Asserts that the code store's head read for the worktree `root` overlays `differing` files and
/// that each node of `DOCS`, composed over it with a doc reader, reads its doc at `column`.
fn assert_docs(root: &Path, state: &str, differing: usize, column: usize) {
    let head = repository_code_store(root)
        .head(root)
        .expect("the head reads");
    assert_eq!(head.compared, Compared::Overlaid(differing), "{state}");
    let lines = WorktreeLines::new(&head.worktree);
    let extract = (head.extract.as_deref()).expect("the head carries the store's extractor");
    let records = Indexed::default();
    let filter = ViewFilter {
        tests: Tests::Include,
    };
    let docs = DocReader {
        lines: &lines,
        extract,
    };
    let graph = Composed::near(
        &records,
        &*head.view,
        filter,
        head_honesty(&head),
        Some(docs),
    )
    .expect("the head composes");
    for (id, want) in DOCS {
        let seed = [id.to_string()];
        let node = (graph.subgraph(&seed, 0).expect("the graph reads").nodes)
            .into_iter()
            .find(|n| n.id == *id)
            .unwrap_or_else(|| panic!("{state}: the graph holds no {id}"));
        assert_eq!(graph.doc(&node).as_deref(), want[column], "{state}: {id}");
    }
}
