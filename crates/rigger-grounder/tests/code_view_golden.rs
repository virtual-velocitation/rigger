//! The resolver's golden: a fixture tree holding a row for every rule of the resolver's table,
//! and the graph the retired code fold made of it, kept as `golden/code_view.golden`. The code
//! view answers it, save the departures the code store states.

#![cfg(all(feature = "symbols", any(feature = "store", not(feature = "core"))))]

extern crate rigger_grounder as rigger;

use std::path::{Path, PathBuf};

use rigger_domain::code_facts::{Stamp, Tip};
use rigger_domain::code_graph::CodeGraph;
use rigger_domain::code_view::{Extract, FactsView, ViewFilter};
use rigger_domain::contextgraph::Graph;
use rigger_grounder::facts::FactsExtractor;

#[path = "../../../tests/common/fixtures/resolver_tree.rs"]
mod resolver_tree;
#[path = "../../../tests/common/symbol_index.rs"]
mod symbol_index;

use resolver_tree::RESOLVER_TREE as TREE;

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

/// The view of the tree at `root`: every file's facts, read by the production extractor.
fn viewed(root: &Path) -> FactsView {
    let facts = TREE
        .iter()
        .map(|(path, _)| {
            let bytes = std::fs::read(root.join(path)).unwrap();
            symbol_index::facts_of(&FactsExtractor, path, &bytes)
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
const DEPARTURES: &[(&str, &[&str], &[&str])] = &[
    (
        "a test call from a file in the test-module closure proves the product definition it calls",
        &[
            "node src/shapes.rs::origin [code-entity] kind=function lang=rust line=16 name=origin",
            "node src/util.rs::compute [code-entity] kind=function lang=rust line=3 name=compute",
        ],
        &[
            r#"node src/shapes.rs::origin [code-entity] kind=function lang=rust line=16 name=origin proof_evidence=["src/lib_tests/more.rs:2"] proven_by=1"#,
            r#"node src/util.rs::compute [code-entity] kind=function lang=rust line=3 name=compute proof_evidence=["src/lib_tests.rs:5"] proven_by=1"#,
        ],
    ),
    (
        "a file no declaration reaches places its modules as a crate root does (spec 117, MODULE \
         ROOTS AT READ)",
        &[
            "edge src/twins.rs::left::leaf -CONTAINS/extracted-> src/twins/left/leaf.rs",
            "edge src/twins.rs::right::leaf -CONTAINS/extracted-> src/twins/right/leaf.rs",
        ],
        &[],
    ),
    (
        "a call inside a macro's token tree is read, so a test's call inside `assert_eq!` proves \
         the product definition it calls (spec 117, MACRO TOKEN TREES)",
        &[
            r#"node src/lib.rs::helper [code-entity] kind=function lang=rust line=26 name=helper proof_evidence=["src/lib.rs:39"] proven_by=1"#,
        ],
        &[
            r#"node src/lib.rs::helper [code-entity] kind=function lang=rust line=26 name=helper proof_evidence=["src/lib.rs:39","src/lib.rs:40"] proven_by=2"#,
        ],
    ),
    (
        "an impl header's trait and self type are `impl` mentions the impl encloses, never calls, \
         and a prelude trait such as `Default` stages none (spec 117, RUST TAGS IMPL HEADERS)",
        &[
            "edge src/shapes.rs -REFERENCES/ambiguous-> src/shapes.rs::%Default",
            "edge src/shapes.rs::impl Default for Shape -CALLS/ambiguous-> src/shapes.rs::%Default",
            "edge src/shapes.rs::impl Default for Shape -CALLS/extracted-> src/shapes.rs::Shape",
            "edge src/shapes.rs::impl Shape -CALLS/extracted-> src/shapes.rs::Shape",
            "node src/shapes.rs::%Default [code-entity] ",
        ],
        &[
            "edge src/shapes.rs::impl Default for Shape -REFERENCES/extracted-> src/shapes.rs::Shape",
            "edge src/shapes.rs::impl Shape -REFERENCES/extracted-> src/shapes.rs::Shape",
        ],
    ),
    (
        "a call qualified by a type reaches the method the type's impl defines, the impl \
         mentioning its self type (spec 117, RUST TAGS IMPL HEADERS)",
        &[
            "edge src/lib.rs -REFERENCES/inferred-> src/lib.rs::%new",
            "edge src/lib.rs::run -CALLS/inferred-> src/lib.rs::%new",
            "node src/lib.rs::%new [code-entity] ",
        ],
        &[
            "edge src/lib.rs -REFERENCES/extracted-> src/shapes.rs::Shape::new",
            "edge src/lib.rs::run -CALLS/extracted-> src/shapes.rs::Shape::new",
        ],
    ),
];

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
