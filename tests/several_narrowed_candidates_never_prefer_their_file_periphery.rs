//! Periphery (API / integration) tests for spec 117, SEVERAL NARROWED CANDIDATES NEVER PREFER
//! THEIR FILE: a call whose qualifier keeps several candidates lands on the placeholder keyed by
//! its qualified name (`F::%CodeView.file`), never on the candidate in its own file, and that
//! placeholder reaches the kept candidates alone. The resolver's own test reads one such call
//! from inside the crate; these plant the facts as an extractor would read them and run OUTSIDE
//! the crates over both code halves the composition root wires, the whole one
//! (`Composed::new`) and the one a lookup reads (`Composed::near`), where several placeholders of
//! one leaf meet:
//!
//!  - the call walk down, where each placeholder is a frontier of what its own qualifier keeps,
//!    in the product pool and, with tests shown, in the test pool beside it;
//!  - the call walk up, where a definition's callers are those whose placeholder keeps it, each
//!    under its own placeholder's frontier, beside an unqualified caller of the leaf;
//!  - the hidden test callers, filed under the candidates a test call's qualifier keeps;
//!  - the files lens over a half's whole graph, which binds each placeholder by its leaf over the
//!    candidates it carries, whole and re-grained.

use crate::common;

use common::fixtures::{
    call_edge_pairs, call_frontiers, call_node_ids, call_walk, define_member, facts_view, halves,
    one_community_graph, owned_ids, planted_file, with_calls, with_tagged, ONE_COMMUNITY,
};
use common::lens::files_lens_rows;
use rigger::code_facts::FileFacts;
use rigger::code_view::{FactsView, Tests};
use rigger::composed::Indexed;
use rigger::contextgraph::query::leaf_of;
use rigger::contextgraph::{
    Direction, Graph, Node, CODE_KIND_FUNCTION, CODE_KIND_IMPL, CODE_KIND_METHOD, CODE_KIND_TRAIT,
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
/// `Local`'s default `file`, a trait `Mirror` implements.
const LOCAL_FILE: &str = "src/mirror.rs::Local::file";
/// `Remote`'s default `file`, the other trait `Mirror` implements.
const REMOTE_FILE: &str = "src/mirror.rs::Remote::file";
/// The function calling `CodeView.file`, `Archive.file` and `Mirror.file`.
const SUMMARY: &str = "src/report.rs::summary";
/// The function calling `file` unqualified.
const LISTING: &str = "src/report.rs::listing";
/// The test calling `Archive.file`, in the file of the test implementation `Fake`.
const CHECKS: &str = "tests/report.rs::checks";
/// The test implementation `Fake`'s `file`, implementing `Archive`.
const FAKE_FILE: &str = "tests/report.rs::Fake::file";
/// The test calling `Archive.file` from a file holding no definition of `file`.
const AUDIT: &str = "tests/audit.rs::audit";

/// What `CodeView.file` keeps: both implementations of `CodeView`.
const CODE_VIEWS: &[&str] = &[FACTS_VIEW_FILE, SNAPSHOT_FILE];
/// What `Archive.file` keeps in the product pool: the product implementations of `Archive`.
const ARCHIVES: &[&str] = &[TARBALL_FILE, ZIP_FILE];
/// What `Archive.file` keeps with tests shown: every implementation of `Archive`.
const ARCHIVES_SHOWN: &[&str] = &[TARBALL_FILE, ZIP_FILE, FAKE_FILE];
/// What `Mirror.file` keeps in the product pool: both traits' default methods.
const MIRRORS: &[&str] = &[LOCAL_FILE, REMOTE_FILE];
/// What `Mirror.file` keeps with tests shown: the test type's own `file` beside them.
const MIRRORS_SHOWN: &[&str] = &[LOCAL_FILE, REMOTE_FILE, TEST_MIRROR_FILE];
/// Every product definition of `file`, in id order.
const EVERY: &[&str] = &[
    TARBALL_FILE,
    ZIP_FILE,
    FACTS_VIEW_FILE,
    LOCAL_FILE,
    REMOTE_FILE,
    SNAPSHOT_FILE,
    STORE_FILE,
];
/// Every definition of `file`, test ones included, in id order.
const EVERY_SHOWN: &[&str] = &[
    TARBALL_FILE,
    ZIP_FILE,
    FACTS_VIEW_FILE,
    LOCAL_FILE,
    REMOTE_FILE,
    SNAPSHOT_FILE,
    STORE_FILE,
    FAKE_FILE,
    TEST_MIRROR_FILE,
];
/// The test type `Mirror`'s own `file`: what a `Mirror.file` call keeps in the test pool, where
/// the product `Mirror` holds no `file` of its own and the call keeps its traits' methods.
const TEST_MIRROR_FILE: &str = "tests/report.rs::Mirror::file";

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
///  - `src/mirror.rs` holds the traits `Local` and `Remote`, each with a default `file`, and the
///    type `Mirror` implementing both, with no `file` of its own;
///  - `src/report.rs` holds `summary`, calling `CodeView.file`, `Archive.file` and `Mirror.file`,
///    and `listing`, calling `file` unqualified;
///  - `tests/report.rs` holds the test `checks`, calling `Archive.file`, beside the test
///    implementation `Fake` of `Archive` and the test type `Mirror` with its own `file`;
///  - `tests/audit.rs` holds the test `audit`, calling `Archive.file` from a file holding no
///    definition of `file`.
///
/// Each implementation of `CodeView` and `Archive` holds its own `file`.
fn several_traits_sharing_a_leaf() -> FactsView {
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
    let mut mirror = planted_file(
        "src/mirror.rs",
        &[
            ("Local", CODE_KIND_TRAIT),
            ("Remote", CODE_KIND_TRAIT),
            ("Mirror", CODE_KIND_TYPE),
            ("impl Local for Mirror", CODE_KIND_IMPL),
            ("impl Remote for Mirror", CODE_KIND_IMPL),
        ],
        &[],
        &[],
    );
    define_member(&mut mirror, "Local", "file", CODE_KIND_METHOD, 6);
    define_member(&mut mirror, "Remote", "file", CODE_KIND_METHOD, 7);
    let mirror = with_tagged(
        mirror,
        REF_KIND_IMPL,
        &[
            ("impl Local for Mirror", "Local"),
            ("impl Local for Mirror", "Mirror"),
            ("impl Remote for Mirror", "Remote"),
            ("impl Remote for Mirror", "Mirror"),
        ],
    );
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
            ("summary", "Mirror.file", "file"),
        ],
    );
    let mut checks = planted_file(
        "tests/report.rs",
        &[
            ("checks", CODE_KIND_FUNCTION),
            ("Fake", CODE_KIND_IMPL),
            ("Mirror", CODE_KIND_TYPE),
        ],
        &[],
        &[],
    );
    define_member(&mut checks, "Mirror", "file", CODE_KIND_METHOD, 4);
    let checks = with_calls(checks, &[("checks", "Archive.file", "file")]);
    let checks = implementing(checks, "Archive", &["Fake"]);
    let audit = planted_file("tests/audit.rs", &[("audit", CODE_KIND_FUNCTION)], &[], &[]);
    let audit = with_calls(audit, &[("audit", "Archive.file", "file")]);
    facts_view(vec![
        code_view, snapshot, archive, store, mirror, report, checks, audit,
    ])
}

/// One call walk and what it reaches: what the walk shows, the tests shown, the seed, the
/// direction, its nodes, its frontiers with their candidates, and its edges' ends.
type Walk<'a> = (
    &'a str,
    Tests,
    &'a str,
    Direction,
    &'a [&'a str],
    &'a [(&'a str, &'a [&'a str])],
    &'a [(&'a str, &'a str)],
);

/// GIVEN the tree of [`several_traits_sharing_a_leaf`] and no records, WHEN the call walk runs
/// down from a caller of `file` or up from a definition of it, on either code half, THEN each
/// qualified call reaches the placeholder keyed by the name it writes, never the implementation
/// in its own file, and that placeholder is a frontier of what its own qualifier keeps alone: in
/// the product pool, and, with tests shown, in the test pool beside it, never in its place; a
/// definition's callers are those whose placeholder keeps it, each under that placeholder's
/// frontier; the unqualified call reaches every definition of the leaf; and with only tests
/// shown, a test call whose qualifier keeps one shown definition reaches it.
#[test]
fn each_placeholder_of_a_leaf_reaches_what_its_own_qualifier_keeps_on_either_half() {
    let records = Indexed::default();
    let view = several_traits_sharing_a_leaf();
    let (code_view, archive, mirror, every) = (CODE_VIEWS, ARCHIVES, MIRRORS, EVERY);
    let (archive_shown, mirror_shown, every_shown) = (ARCHIVES_SHOWN, MIRRORS_SHOWN, EVERY_SHOWN);
    let walks: [Walk; 13] = [
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
            "a function calling three qualifiers' `file`: one frontier per qualifier, a type \
             with no `file` of its own under its traits' methods",
            Tests::Exclude,
            SUMMARY,
            Direction::Down,
            &[
                "src/report.rs::%Archive.file",
                "src/report.rs::%CodeView.file",
                "src/report.rs::%Mirror.file",
                SUMMARY,
            ],
            &[
                ("src/report.rs::%Archive.file", archive),
                ("src/report.rs::%CodeView.file", code_view),
                ("src/report.rs::%Mirror.file", mirror),
            ],
            &[
                (SUMMARY, "src/report.rs::%Archive.file"),
                (SUMMARY, "src/report.rs::%CodeView.file"),
                (SUMMARY, "src/report.rs::%Mirror.file"),
            ],
        ),
        (
            "the same function, tests shown: each frontier gains what its qualifier keeps in the \
             test pool beside what it keeps in the product pool, never in its place",
            Tests::Include,
            SUMMARY,
            Direction::Down,
            &[
                "src/report.rs::%Archive.file",
                "src/report.rs::%CodeView.file",
                "src/report.rs::%Mirror.file",
                SUMMARY,
            ],
            &[
                ("src/report.rs::%Archive.file", archive_shown),
                ("src/report.rs::%CodeView.file", code_view),
                ("src/report.rs::%Mirror.file", mirror_shown),
            ],
            &[
                (SUMMARY, "src/report.rs::%Archive.file"),
                (SUMMARY, "src/report.rs::%CodeView.file"),
                (SUMMARY, "src/report.rs::%Mirror.file"),
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
            "the callers of a trait's default method: the `Mirror.file` caller under its \
             frontier of both traits' methods, the unqualified one under every definition",
            Tests::Exclude,
            LOCAL_FILE,
            Direction::Up,
            &[LOCAL_FILE, LISTING, SUMMARY],
            &[(LISTING, every), (SUMMARY, mirror)],
            &[(LISTING, LOCAL_FILE), (SUMMARY, LOCAL_FILE)],
        ),
        (
            "the same callers, tests shown: the `Mirror.file` caller under its frontier holding \
             the test type's own `file` beside both traits' methods",
            Tests::Include,
            LOCAL_FILE,
            Direction::Up,
            &[LOCAL_FILE, LISTING, SUMMARY],
            &[(LISTING, every_shown), (SUMMARY, mirror_shown)],
            &[(LISTING, LOCAL_FILE), (SUMMARY, LOCAL_FILE)],
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
        (
            "the callers of the test implementation, tests shown: each `Archive.file` caller, \
             product and test, under its placeholder's frontier of every implementation, the \
             unqualified one under every definition",
            Tests::Include,
            FAKE_FILE,
            Direction::Up,
            &[LISTING, SUMMARY, AUDIT, FAKE_FILE, CHECKS],
            &[
                (LISTING, every_shown),
                (SUMMARY, archive_shown),
                (AUDIT, archive_shown),
                (CHECKS, archive_shown),
            ],
            &[
                (LISTING, FAKE_FILE),
                (SUMMARY, FAKE_FILE),
                (AUDIT, FAKE_FILE),
                (CHECKS, FAKE_FILE),
            ],
        ),
        (
            "a test call with only tests shown: the one shown implementation its qualifier \
             keeps, never the test type `Mirror`'s own `file` beside it",
            Tests::Only,
            CHECKS,
            Direction::Down,
            &[FAKE_FILE, CHECKS],
            &[],
            &[(CHECKS, FAKE_FILE)],
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

/// GIVEN the tree of [`several_traits_sharing_a_leaf`] and no records, with tests hidden, WHEN each
/// definition of `file` asks for its hidden test callers on either code half, THEN the tests
/// calling `Archive.file`, from the test implementation's file and from a file holding none, are
/// the hidden callers of each product implementation of `Archive` and of no other definition of
/// the leaf.
#[test]
fn a_hidden_test_call_is_filed_under_what_its_own_qualifier_keeps_on_either_half() {
    let records = Indexed::default();
    let view = several_traits_sharing_a_leaf();
    // (the definition, its hidden test callers)
    let cases: [(&str, &[&str]); 7] = [
        (TARBALL_FILE, &[AUDIT, CHECKS]),
        (ZIP_FILE, &[AUDIT, CHECKS]),
        (FACTS_VIEW_FILE, &[]),
        (SNAPSHOT_FILE, &[]),
        (STORE_FILE, &[]),
        (LOCAL_FILE, &[]),
        (REMOTE_FILE, &[]),
    ];
    for (half, graph) in halves(&records, &view, Tests::Exclude) {
        for (id, want) in cases {
            let hidden = graph.hidden_placeholder_callers(&[id.to_string()]);
            assert_eq!(hidden.unwrap(), want, "{half}: {id}");
        }
    }
}

/// Whether `node` is a placeholder: its id names a leaf ([`leaf_of`]).
fn is_placeholder(node: &Node) -> bool {
    leaf_of(&node.id).is_some()
}

/// `whole`, a code half's whole graph, with its placeholders ([`is_placeholder`]) made the members
/// of [`ONE_COMMUNITY`]: the subject the files lens re-grains.
fn placeholders_in_one_community(whole: Graph) -> Graph {
    let (placeholders, rest): (Vec<Node>, Vec<Node>) =
        (whole.nodes.into_iter()).partition(is_placeholder);
    let mut graph = one_community_graph(ONE_COMMUNITY, placeholders);
    graph.nodes.extend(rest);
    graph.edges.extend(whole.edges);
    graph
}

/// The `candidates` attribute a placeholder whose down-hop keeps `ids` carries: present, their
/// JSON array.
fn carried(ids: &[&str]) -> Option<String> {
    let quoted: Vec<String> = ids.iter().map(|id| format!("\"{id}\"")).collect();
    Some(format!("[{}]", quoted.join(",")))
}

/// One filter's reading of the placeholders: what the reading shows, the tests shown, each
/// placeholder with the `candidates` it carries, and the files lens rows ([`files_lens_rows`]) of
/// the community of them.
type Carried<'a> = (&'a str, Tests, Vec<(&'a str, Option<String>)>, Vec<String>);

/// GIVEN the tree of [`several_traits_sharing_a_leaf`] and no records, composed under each tests
/// filter by either code half, WHEN the files lens folds the half's whole graph and re-grains the
/// community of its placeholders, THEN each placeholder keyed by a qualified name carries the ids
/// its down-hop keeps as its `candidates`, and the lens binds it by its leaf over those alone:
/// several shown are a frontier of them, never every definition of the leaf, nor counted under any
/// file; one shown folds under its file, carrying its caller's edge there; the unqualified
/// placeholder carries none and is a frontier of every shown definition of the leaf.
#[test]
fn the_files_lens_binds_each_qualified_placeholder_over_the_down_hop_it_carries() {
    let records = Indexed::default();
    let view = several_traits_sharing_a_leaf();
    let (described, code_view) = (
        "src/code_view.rs::%CodeView.file",
        "src/report.rs::%CodeView.file",
    );
    let (archive, mirror, file) = (
        "src/report.rs::%Archive.file",
        "src/report.rs::%Mirror.file",
        "src/report.rs::%file",
    );
    let (audit, checks) = (
        "tests/audit.rs::%Archive.file",
        "tests/report.rs::%Archive.file",
    );
    let row = |text: &str| text.to_string();
    let frontier = |id: &str, cands: &[&str]| format!("concept {id} <- {cands:?}");
    let cases: [Carried; 3] = [
        (
            "tests hidden: each qualified placeholder a frontier of what its qualifier keeps in \
             the product pool, the unqualified one of every product definition",
            Tests::Exclude,
            vec![
                (described, carried(CODE_VIEWS)),
                (archive, carried(ARCHIVES)),
                (code_view, carried(CODE_VIEWS)),
                (mirror, carried(MIRRORS)),
                (file, None),
            ],
            vec![
                row("whole src/archive.rs 5"),
                row("whole src/code_view.rs 4"),
                row("whole src/mirror.rs 7"),
                row("whole src/report.rs 2"),
                row("whole src/snapshot.rs 2"),
                row("whole src/store.rs 2"),
                row("whole src/code_view.rs -- src/snapshot.rs 1"),
                frontier(described, CODE_VIEWS),
                frontier(archive, ARCHIVES),
                frontier(code_view, CODE_VIEWS),
                frontier(mirror, MIRRORS),
                frontier(file, EVERY),
            ],
        ),
        (
            "tests shown: each qualified placeholder a frontier of what its qualifier keeps in \
             the test pool beside the product pool, the unqualified one of every definition",
            Tests::Include,
            vec![
                (described, carried(CODE_VIEWS)),
                (archive, carried(ARCHIVES_SHOWN)),
                (code_view, carried(CODE_VIEWS)),
                (mirror, carried(MIRRORS_SHOWN)),
                (file, None),
                (audit, carried(ARCHIVES_SHOWN)),
                (checks, carried(ARCHIVES_SHOWN)),
            ],
            vec![
                row("whole src/archive.rs 5"),
                row("whole src/code_view.rs 4"),
                row("whole src/mirror.rs 7"),
                row("whole src/report.rs 2"),
                row("whole src/snapshot.rs 2"),
                row("whole src/store.rs 2"),
                row("whole tests/audit.rs 1"),
                row("whole tests/report.rs 5"),
                row("whole src/archive.rs -- tests/report.rs 1"),
                row("whole src/code_view.rs -- src/snapshot.rs 1"),
                frontier(described, CODE_VIEWS),
                frontier(archive, ARCHIVES_SHOWN),
                frontier(code_view, CODE_VIEWS),
                frontier(mirror, MIRRORS_SHOWN),
                frontier(file, EVERY_SHOWN),
                frontier(audit, ARCHIVES_SHOWN),
                frontier(checks, ARCHIVES_SHOWN),
            ],
        ),
        (
            "only tests shown: each test placeholder carrying every implementation keeps the one \
             shown, never the test type `Mirror`'s own `file`, and folds under its file, the \
             call from a file holding none coupling the two files",
            Tests::Only,
            vec![
                (audit, carried(ARCHIVES_SHOWN)),
                (checks, carried(ARCHIVES_SHOWN)),
            ],
            vec![
                row("whole src/archive.rs 1"),
                row("whole tests/audit.rs 1"),
                row("whole tests/report.rs 5"),
                row("whole src/archive.rs -- tests/report.rs 1"),
                row("whole tests/audit.rs -- tests/report.rs 1"),
                row("concept tests/report.rs 2"),
            ],
        ),
    ];
    for (shows, tests, placeholders, rows) in cases {
        for (half, composed) in halves(&records, &view, tests) {
            let graph = placeholders_in_one_community(composed.whole().unwrap());
            let read: Vec<(&str, Option<String>)> = (graph.nodes.iter())
                .filter(|n| is_placeholder(n))
                .map(|n| (n.id.as_str(), n.attrs.get("candidates").cloned()))
                .collect();
            assert_eq!(
                (
                    read,
                    files_lens_rows(&graph, ONE_COMMUNITY, placeholders.len())
                ),
                (placeholders.clone(), rows.clone()),
                "{half}: {shows}"
            );
        }
    }
}
