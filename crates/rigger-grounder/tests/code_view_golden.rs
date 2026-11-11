//! The resolver's golden: a fixture tree holding a row for every rule of the resolver's table,
//! and the graph the code fold made of it, captured once as `golden/code_view.golden`. The fold
//! half proves the golden is the fold's; the view half proves the code view answers it, save
//! the departures the code store states.
//!
//! Capture: `RIGGER_CAPTURE_GOLDEN=1 cargo test -p rigger-grounder --test code_view_golden`
//! rewrites the golden from the fold. A normal run only compares.

#![cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]

use std::path::{Path, PathBuf};

use rigger_domain::code_facts::{Stamp, Tip};
use rigger_domain::code_graph::CodeGraph;
use rigger_domain::code_view::{Entry, Extract, FactsView, ViewFilter};
use rigger_domain::contextgraph::{EntryFold, Graph, Projection};
use rigger_domain::eventstore::Event;
use rigger_domain::ingest::derived_key_parts;
use rigger_domain::retention::GenerationIngested;
use rigger_graph_sqlite::contextgraph::sqlite::Projector;
use rigger_grounder::facts::FactsExtractor;

/// The fixture tree, one `(path, contents)` per file. Each file's comment names the rows of the
/// resolver's table it holds.
const TREE: &[(&str, &str)] = &[
    // Same-file call (helper), cross-file unique call (compute), ambiguous call (draw), a name
    // no definition holds (vanish), type, const and use references, module declarations (one
    // nested in an inline module, one test-only), a nested definition, a test helper sharing a
    // product leaf, and test calls that prove product definitions.
    (
        "src/lib.rs",
        r#"use crate::shapes::Shape;
use crate::util::{compute, LIMIT};

mod shapes;
mod util;
#[cfg(test)]
mod lib_tests;

pub mod inner {
    pub fn deep() -> u32 {
        super::helper()
    }

    mod nested;
}

pub fn run() -> u32 {
    let shape: Shape = Shape::new();
    helper();
    compute();
    draw();
    vanish();
    inner::deep() + LIMIT + shape.0
}

fn helper() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    fn helper() -> u32 {
        2
    }

    #[test]
    fn runs() {
        let total = super::run();
        super::helper();
        assert_eq!(total, helper());
    }
}
"#,
    ),
    // Duplicate ids: the struct and its impls share `Shape`, the cfg twins share `origin`.
    (
        "src/shapes.rs",
        r#"pub struct Shape(pub u32);

impl Shape {
    pub fn new() -> Self {
        Shape(origin())
    }
}

impl Default for Shape {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
pub fn origin() -> u32 {
    0
}

#[cfg(not(unix))]
pub fn origin() -> u32 {
    1
}

pub fn draw() {}
"#,
    ),
    // A same-file const read, the second `draw`, and a field sharing the leaf `draw`, which is
    // no call target.
    (
        "src/util.rs",
        r#"pub const LIMIT: u32 = 3;

pub fn compute() -> u32 {
    LIMIT
}

pub fn draw() {}

pub struct Config {
    pub draw: u32,
}
"#,
    ),
    // The module the inline `inner` declares: a cross-file call whose only product candidate
    // shares its leaf with a test helper.
    (
        "src/inner/nested.rs",
        r#"pub fn nested_value() -> u32 {
    super::super::helper()
}
"#,
    ),
    // The out-of-line test module, the closure's seed, which declares a module of its own.
    (
        "src/lib_tests.rs",
        r#"mod more;

#[test]
fn computes() {
    let value = super::util::compute();
    assert_eq!(value, 3);
}
"#,
    ),
    // A file reached only through a test module: test code by the closure alone.
    (
        "src/lib_tests/more.rs",
        r#"pub fn more_origin() -> u32 {
    crate::shapes::origin()
}
"#,
    ),
    // A Test-scope file by the path rule: a unique proof (run) and an ambiguous one (draw).
    (
        "tests/integration.rs",
        r#"#[test]
fn runs_end_to_end() {
    let total = fixture::run();
    fixture::draw();
    assert_eq!(total, 4);
}
"#,
    ),
    // A second language sharing the leaf `Shape` (a type reference filters on language) and
    // calling `compute` (a call does not).
    (
        "scripts/tool.py",
        r#"class Shape:
    pass


def assist():
    return compute()
"#,
    ),
    // Design links: a design doc specifying a source file and citing an ADR, an ADR
    // constraining one, and a source rationale explaining its file.
    (
        "docs/architecture.md",
        "# Reference architecture\n\n## Shapes\n\nThe shapes live in `src/shapes.rs`.\n\nSee [the decision](adr/0001-shapes.md).\n",
    ),
    (
        "docs/adr/0001-shapes.md",
        "# Shapes are values\n\nThe decision binds `src/util.rs`.\n",
    ),
    // A workflow definition: stages, a gate and an agent.
    (
        ".rigger/workflow.yml",
        "stages:\n  plan:\n    agent: planner\n  implement:\n    agent: worker\n    needs: [plan]\n    gates: [fmt]\ngates:\n  fmt:\n    run: cargo fmt --check\n",
    ),
];

/// Where the golden lives.
fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/code_view.golden")
}

/// The fixture tree planted under a fresh directory.
fn planted() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in TREE {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    dir
}

/// The graph the code fold makes of the tree at `root`: every batch the walk hands, folded with
/// its ledger entry at the next log position.
fn folded(root: &Path) -> Graph {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut position = 0u64;
    rigger_grounder::ingest::ingest_project_batched(
        root.to_str().unwrap(),
        &[],
        |keyed: &[(String, &Event)], excluded: bool| {
            let (identity, generation) = derived_key_parts(&keyed[0].0).unwrap();
            let (prefix, file) = GenerationIngested::identity_parts(identity).unwrap();
            position += 1;
            let mut entry = GenerationIngested {
                prefix: prefix.to_string(),
                file: file.to_string(),
                generation: generation.to_string(),
                blob: String::new(),
                excluded,
            }
            .event(keyed.len());
            entry.position = position;
            let batch: Vec<Event> = keyed.iter().map(|(_, e)| (*e).clone()).collect();
            let fold = graph
                .apply_generation(&entry, Box::new(move || Ok(Some(batch))))
                .unwrap();
            assert_eq!(fold, EntryFold::BatchAsked, "every entry's batch folds");
        },
    );
    graph.whole().unwrap()
}

/// `g` as sorted lines: one per node (id, kind, attributes) and one per live edge (ends,
/// relation, tier). Log positions and valid times are not part of a code edge's answer.
fn render(g: &Graph) -> String {
    let mut lines: Vec<String> = g
        .nodes
        .iter()
        .map(|n| {
            let attrs: Vec<String> = n.attrs.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("node {} [{}] {}", n.id, n.kind, attrs.join(" "))
        })
        .chain(
            g.edges
                .iter()
                .filter(|e| e.valid_to.is_none())
                .map(|e| format!("edge {} -{}/{}-> {}", e.from, e.rel, e.tier, e.to)),
        )
        .collect();
    lines.sort();
    lines.dedup();
    lines.join("\n") + "\n"
}

/// The golden is the fold's graph of the fixture tree.
#[test]
fn the_golden_is_the_retired_folds_graph_of_the_fixture_tree() {
    let tree = planted();
    let fold = render(&folded(tree.path()));
    if std::env::var_os("RIGGER_CAPTURE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), &fold).unwrap();
    }
    let golden = std::fs::read_to_string(golden_path()).expect("the golden is captured");
    assert_eq!(fold, golden);
}

/// The view of the tree at `root`: every file's facts, read by the production extractor.
fn viewed(root: &Path) -> FactsView {
    let facts = TREE
        .iter()
        .map(|(path, _)| {
            let bytes = std::fs::read(root.join(path)).unwrap();
            let entry = Entry {
                path: path.to_string(),
                blob: String::new(),
            };
            FactsExtractor.extract(&entry, &bytes)
        })
        .collect();
    let stamp = Stamp {
        tip: Tip {
            head: "main".into(),
            commit: None,
            tree: "fixture".into(),
        },
        fingerprint: FactsExtractor.fingerprint(),
        seq: 1,
    };
    FactsView::new(stamp, facts)
}

/// The departures the code store states from the fold's graph: each row names one, with the
/// golden lines it drops and the lines it adds.
const DEPARTURES: &[(&str, &[&str], &[&str])] = &[(
    "a test call from a file in the test-module closure proves the product definition it calls",
    &[
        "node src/shapes.rs::origin [code-entity] kind=function lang=rust line=16 name=origin",
        "node src/util.rs::compute [code-entity] kind=function lang=rust line=3 name=compute",
    ],
    &[
        r#"node src/shapes.rs::origin [code-entity] kind=function lang=rust line=16 name=origin proof_evidence=["src/lib_tests/more.rs:2"] proven_by=1"#,
        r#"node src/util.rs::compute [code-entity] kind=function lang=rust line=3 name=compute proof_evidence=["src/lib_tests.rs:5"] proven_by=1"#,
    ],
)];

/// The golden with every departure applied.
fn departed(golden: &str) -> String {
    let mut lines: Vec<String> = golden.lines().map(str::to_string).collect();
    for (name, dropped, added) in DEPARTURES {
        for line in *dropped {
            let at = lines.iter().position(|l| l == line);
            lines.remove(at.unwrap_or_else(|| panic!("{name}: {line} is not in the golden")));
        }
        lines.extend(added.iter().map(|l| l.to_string()));
    }
    lines.sort();
    lines.join("\n") + "\n"
}

/// The code view, resolved by the default filter, answers the fold's golden, save the stated
/// departures.
#[test]
fn code_view_answers_the_golden_of_the_retired_fold() {
    let tree = planted();
    let view = viewed(tree.path());
    let answer = render(&CodeGraph::new(&view, ViewFilter::default()).whole());
    let golden = std::fs::read_to_string(golden_path()).expect("the golden is captured");
    assert_eq!(answer, departed(&golden));
}
