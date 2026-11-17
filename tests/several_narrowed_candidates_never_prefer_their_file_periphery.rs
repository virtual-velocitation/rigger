//! Periphery (API / integration) tests for spec 117, SEVERAL NARROWED CANDIDATES NEVER PREFER
//! THEIR FILE: a call whose qualifier keeps several candidates lands on the placeholder keyed by
//! its qualified name (`F::%CodeView.file`), never on the candidate in its own file, and that
//! placeholder reaches the kept candidates alone. The resolver's own test reads one such call
//! from inside the crate; these plant the facts as an extractor would read them and run OUTSIDE
//! the crates over both code halves the composition root wires, the whole one
//! (`Composed::new`) and the one a lookup reads (`Composed::near`), where several placeholders of
//! one leaf meet:
//!
//!  - the call walk down, where each placeholder is a frontier of what its own qualifier keeps;
//!  - the call walk up, where a definition's callers are those whose placeholder keeps it, each
//!    under its own placeholder's frontier, beside an unqualified caller of the leaf;
//!  - the hidden test callers, filed under the candidates a test call's qualifier keeps.

use crate::common;

use common::fixtures::{
    call_edge_pairs, call_frontiers, call_node_ids, call_walk, define_member, facts_view,
    matching_honesty, owned_ids, planted_file, with_calls, with_tagged,
};
use rigger::code_facts::FileFacts;
use rigger::code_view::{FactsView, Tests, ViewFilter};
use rigger::composed::{Composed, Indexed};
use rigger::contextgraph::{
    ContextGraph, Direction, CODE_KIND_FUNCTION, CODE_KIND_IMPL, CODE_KIND_METHOD, CODE_KIND_TRAIT,
    CODE_KIND_TYPE, REF_KIND_IMPL,
};

/// `CodeView`'s default method, which calls `CodeView.file`.
const DESCRIBE: &str = "src/code_view.rs::CodeView::describe";
/// `FactsView`'s `file`, implementing `CodeView` in the file [`DESCRIBE`] calls it from.
const FACTS_VIEW_FILE: &str = "src/code_view.rs::FactsView::file";
/// `Snapshot`'s `file`, implementing `CodeView` in another file.
const SNAPSHOT_FILE: &str = "src/snapshot.rs::Snapshot::file";
/// `Tarball`'s `file`, implementing `Archive`.
const TARBALL_FILE: &str = "src/archive.rs::Tarball::file";
/// `Zip`'s `file`, implementing `Archive`.
const ZIP_FILE: &str = "src/archive.rs::Zip::file";
/// `Store`'s own `file`, implementing no trait.
const STORE_FILE: &str = "src/store.rs::Store::file";
/// The function calling `CodeView.file` and `Archive.file`.
const SUMMARY: &str = "src/report.rs::summary";
/// The function calling `file` unqualified.
const LISTING: &str = "src/report.rs::listing";
/// The test calling `Archive.file`, in the file of the test implementation `Fake`.
const CHECKS: &str = "tests/report.rs::checks";
/// The test implementation `Fake`'s `file`, implementing `Archive`.
const FAKE_FILE: &str = "tests/report.rs::Fake::file";

/// `file`, whose `impl` blocks named `types` it already defines, with each block's header naming
/// `trait_name` (its `impl` mention) and the block holding the method `file`.
fn implementing(mut file: FileFacts, trait_name: &str, types: &[&str]) -> FileFacts {
    for ty in types {
        let line = file.defs.len() as u32 + 1;
        define_member(&mut file, ty, "file", CODE_KIND_METHOD, line);
    }
    let headers: Vec<(&str, &str)> = types.iter().map(|ty| (*ty, trait_name)).collect();
    with_tagged(file, REF_KIND_IMPL, &headers)
}

/// The code store's view of a tree where two traits share the method leaf `file`:
///
///  - `src/code_view.rs` holds the trait `CodeView`, whose default method `describe` calls
///    `CodeView.file`, and `FactsView`'s implementation of it;
///  - `src/snapshot.rs` holds `Snapshot`'s implementation of `CodeView`;
///  - `src/archive.rs` holds the trait `Archive` and `Tarball`'s and `Zip`'s implementations of it;
///  - `src/store.rs` holds the type `Store` and its own method `file`;
///  - `src/report.rs` holds `summary`, calling `CodeView.file` and `Archive.file`, and `listing`,
///    calling `file` unqualified;
///  - `tests/report.rs` holds the test `checks`, calling `Archive.file`, beside the test
///    implementation `Fake` of `Archive`.
///
/// Each implementation holds its own `file`.
fn two_traits_sharing_a_leaf() -> FactsView {
    let mut code_view = planted_file(
        "src/code_view.rs",
        &[("CodeView", CODE_KIND_TRAIT), ("FactsView", CODE_KIND_IMPL)],
        &[],
        &[],
    );
    define_member(&mut code_view, "CodeView", "describe", CODE_KIND_METHOD, 3);
    let code_view = with_calls(code_view, &[("describe", "CodeView.file", "file")]);
    let code_view = implementing(code_view, "CodeView", &["FactsView"]);
    let snapshot = planted_file("src/snapshot.rs", &[("Snapshot", CODE_KIND_IMPL)], &[], &[]);
    let snapshot = implementing(snapshot, "CodeView", &["Snapshot"]);
    let archive = planted_file(
        "src/archive.rs",
        &[
            ("Archive", CODE_KIND_TRAIT),
            ("Tarball", CODE_KIND_IMPL),
            ("Zip", CODE_KIND_IMPL),
        ],
        &[],
        &[],
    );
    let archive = implementing(archive, "Archive", &["Tarball", "Zip"]);
    let mut store = planted_file("src/store.rs", &[("Store", CODE_KIND_TYPE)], &[], &[]);
    define_member(&mut store, "Store", "file", CODE_KIND_METHOD, 2);
    let report = planted_file(
        "src/report.rs",
        &[
            ("summary", CODE_KIND_FUNCTION),
            ("listing", CODE_KIND_FUNCTION),
        ],
        &[("listing", "file")],
        &[],
    );
    let report = with_calls(
        report,
        &[
            ("summary", "CodeView.file", "file"),
            ("summary", "Archive.file", "file"),
        ],
    );
    let checks = planted_file(
        "tests/report.rs",
        &[("checks", CODE_KIND_FUNCTION), ("Fake", CODE_KIND_IMPL)],
        &[],
        &[],
    );
    let checks = with_calls(checks, &[("checks", "Archive.file", "file")]);
    let checks = implementing(checks, "Archive", &["Fake"]);
    facts_view(vec![code_view, snapshot, archive, store, report, checks])
}

/// `records` composed with `view` under `tests` by each code half the composition root wires,
/// named: the whole one, and the one resolved only near the ids it is asked about.
fn halves<'r>(
    records: &'r Indexed,
    view: &'r FactsView,
    tests: Tests,
) -> [(&'static str, Box<dyn ContextGraph + 'r>); 2] {
    let filter = ViewFilter { tests };
    let whole = Composed::new(records, view, filter, matching_honesty()).unwrap();
    let near = Composed::near(records, view, filter, matching_honesty()).unwrap();
    [("whole", Box::new(whole)), ("near", Box::new(near))]
}

/// GIVEN the tree of [`two_traits_sharing_a_leaf`] and no records, WHEN the call walk runs down
/// from a caller of `file` or up from a definition of it, on either code half, THEN each
/// qualified call reaches the placeholder keyed by the name it writes, never the implementation
/// in its own file, and that placeholder is a frontier of its own trait's implementations alone;
/// a definition's callers are those whose placeholder keeps it, each under that placeholder's
/// frontier; and the unqualified call reaches every definition of the leaf.
#[test]
fn each_placeholder_of_a_leaf_reaches_what_its_own_qualifier_keeps_on_either_half() {
    let records = Indexed::default();
    let view = two_traits_sharing_a_leaf();
    let code_view: &[&str] = &[FACTS_VIEW_FILE, SNAPSHOT_FILE];
    let archive: &[&str] = &[TARBALL_FILE, ZIP_FILE];
    let every: &[&str] = &[
        TARBALL_FILE,
        ZIP_FILE,
        FACTS_VIEW_FILE,
        SNAPSHOT_FILE,
        STORE_FILE,
    ];
    let archive_shown: &[&str] = &[TARBALL_FILE, ZIP_FILE, FAKE_FILE];
    // (what the walk shows, the tests shown, the seed, the direction, its nodes, its frontiers
    // with their candidates, its edges' ends)
    #[allow(clippy::type_complexity)]
    let walks: [(
        &str,
        Tests,
        &str,
        Direction,
        &[&str],
        &[(&str, &[&str])],
        &[(&str, &str)],
    ); 8] = [
        (
            "the trait's method calling `CodeView.file`: both implementations, the one in its \
             own file among them",
            Tests::Exclude,
            DESCRIBE,
            Direction::Down,
            &["src/code_view.rs::%CodeView.file", DESCRIBE],
            &[("src/code_view.rs::%CodeView.file", code_view)],
            &[(DESCRIBE, "src/code_view.rs::%CodeView.file")],
        ),
        (
            "a function calling both traits' `file`: one frontier per qualifier",
            Tests::Exclude,
            SUMMARY,
            Direction::Down,
            &[
                "src/report.rs::%Archive.file",
                "src/report.rs::%CodeView.file",
                SUMMARY,
            ],
            &[
                ("src/report.rs::%Archive.file", archive),
                ("src/report.rs::%CodeView.file", code_view),
            ],
            &[
                (SUMMARY, "src/report.rs::%Archive.file"),
                (SUMMARY, "src/report.rs::%CodeView.file"),
            ],
        ),
        (
            "an unqualified call of the leaf: every definition of it",
            Tests::Exclude,
            LISTING,
            Direction::Down,
            &["src/report.rs::%file", LISTING],
            &[("src/report.rs::%file", every)],
            &[(LISTING, "src/report.rs::%file")],
        ),
        (
            "the callers of the implementation in the call's own file: each `CodeView.file` \
             caller under its trait's frontier, the unqualified one under every definition",
            Tests::Exclude,
            FACTS_VIEW_FILE,
            Direction::Up,
            &[DESCRIBE, FACTS_VIEW_FILE, LISTING, SUMMARY],
            &[
                (DESCRIBE, code_view),
                (LISTING, every),
                (SUMMARY, code_view),
            ],
            &[
                (DESCRIBE, FACTS_VIEW_FILE),
                (LISTING, FACTS_VIEW_FILE),
                (SUMMARY, FACTS_VIEW_FILE),
            ],
        ),
        (
            "the callers of the implementation in another file, the same three",
            Tests::Exclude,
            SNAPSHOT_FILE,
            Direction::Up,
            &[DESCRIBE, LISTING, SUMMARY, SNAPSHOT_FILE],
            &[
                (DESCRIBE, code_view),
                (LISTING, every),
                (SUMMARY, code_view),
            ],
            &[
                (DESCRIBE, SNAPSHOT_FILE),
                (LISTING, SNAPSHOT_FILE),
                (SUMMARY, SNAPSHOT_FILE),
            ],
        ),
        (
            "the callers of the other trait's implementation: the `Archive.file` caller under \
             its frontier, never the trait method calling `CodeView.file`",
            Tests::Exclude,
            TARBALL_FILE,
            Direction::Up,
            &[TARBALL_FILE, LISTING, SUMMARY],
            &[(LISTING, every), (SUMMARY, archive)],
            &[(LISTING, TARBALL_FILE), (SUMMARY, TARBALL_FILE)],
        ),
        (
            "the callers of a method implementing no trait: the unqualified caller alone",
            Tests::Exclude,
            STORE_FILE,
            Direction::Up,
            &[LISTING, STORE_FILE],
            &[(LISTING, every)],
            &[(LISTING, STORE_FILE)],
        ),
        (
            "a shown test call keeping the test implementation in its own file: the qualified \
             placeholder, a frontier of every implementation",
            Tests::Include,
            CHECKS,
            Direction::Down,
            &["tests/report.rs::%Archive.file", CHECKS],
            &[("tests/report.rs::%Archive.file", archive_shown)],
            &[(CHECKS, "tests/report.rs::%Archive.file")],
        ),
    ];
    for (shows, tests, seed, direction, nodes, frontiers, edges) in walks {
        let frontiers: Vec<(String, Vec<String>)> = (frontiers.iter())
            .map(|(id, cands)| (id.to_string(), owned_ids(cands)))
            .collect();
        let edges: Vec<(String, String)> = (edges.iter())
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect();
        for (half, graph) in halves(&records, &view, tests) {
            let walked = call_walk(&*graph, seed, direction);
            assert_eq!(
                (
                    call_node_ids(&walked),
                    call_frontiers(&walked),
                    call_edge_pairs(&walked)
                ),
                (owned_ids(nodes), frontiers.clone(), edges.clone()),
                "{half}: {shows}"
            );
        }
    }
}

/// GIVEN the tree of [`two_traits_sharing_a_leaf`] and no records, with tests hidden, WHEN each
/// definition of `file` asks for its hidden test callers on either code half, THEN the test
/// calling `Archive.file` is the hidden caller of each product implementation of `Archive` and
/// of no other definition of the leaf.
#[test]
fn a_hidden_test_call_is_filed_under_what_its_own_qualifier_keeps_on_either_half() {
    let records = Indexed::default();
    let view = two_traits_sharing_a_leaf();
    // (the definition, its hidden test callers)
    let cases: [(&str, &[&str]); 5] = [
        (TARBALL_FILE, &[CHECKS]),
        (ZIP_FILE, &[CHECKS]),
        (FACTS_VIEW_FILE, &[]),
        (SNAPSHOT_FILE, &[]),
        (STORE_FILE, &[]),
    ];
    for (half, graph) in halves(&records, &view, Tests::Exclude) {
        for (id, want) in cases {
            let hidden = graph.hidden_placeholder_callers(&[id.to_string()]);
            assert_eq!(hidden.unwrap(), want, "{half}: {id}");
        }
    }
}
