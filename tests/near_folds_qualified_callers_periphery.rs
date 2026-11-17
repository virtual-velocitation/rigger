//! Periphery (API / integration) tests for spec 117, NEAR FOLDS QUALIFIED CALLERS: a call written
//! `Q::leaf` or `Q.leaf` whose qualifier keeps one candidate lands on that candidate EXTRACTED from
//! any file, so the neighbourhood a lookup reads (`Composed::near`) must fold the file holding the
//! call and answer what the whole code half (`Composed::new`) answers. The domain's own test reads
//! the path-qualified form through `around` from inside the crate; this one plants the facts as an
//! extractor would read them and runs OUTSIDE the crates over both halves the composition root
//! wires, behaviour first:
//!
//!  - GIVEN a definition `FactsView::new` called as `FactsView::new` and as `FactsView.new` from
//!    two other product files, beside a `Snapshot::new` called as `Snapshot::new` from a third
//!    product file and from a test file;
//!  - WHEN `--around` asks for one of the two `new`s under a tests filter, over either half;
//!  - THEN the neighbourhood lists each caller in another file whose qualifier keeps the seed, by
//!    one EXTRACTED `CALLS` edge, and no other, and the callers walk up reaches exactly those
//!    callers under no frontier.
//!
//! No test calls `FactsView::new`, `Snapshot::new` lies outside its neighbourhood, and
//! `Snapshot::new` is asked about only with tests shown: whether a hidden qualified test call
//! proves the definition it calls is the proof rule's to answer, not the fold's, so no row's
//! hidden-test count turns on it.

use crate::common;

use common::fixtures::{
    call_edge_pairs, call_frontiers, call_node_ids, call_walk, define_member, facts_view, halves,
    planted_file, with_calls,
};
use rigger::code_view::{FactsView, Tests};
use rigger::composed::Indexed;
use rigger::contextgraph::{
    around, Direction, CODE_KIND_FUNCTION, CODE_KIND_METHOD, CODE_KIND_TYPE, REL_CALLS,
    TIER_EXTRACTED,
};

/// `FactsView`'s `new`, the seed the criterion names.
const NEW: &str = "src/code_view.rs::FactsView::new";
/// `Snapshot`'s `new`, the other definition of the leaf `new`.
const SNAPSHOT_NEW: &str = "src/snapshot.rs::Snapshot::new";
/// The method calling `FactsView::new`, path-qualified, from another file.
const OPEN: &str = "src/composed.rs::Composed::open";
/// The function calling `FactsView.new`, member-qualified, from another file.
const SUMMARY: &str = "src/report.rs::summary";
/// The function calling `Snapshot::new` from another file.
const AUDIT: &str = "src/audit.rs::audit";
/// The test calling `Snapshot::new` from another file.
const OPENS: &str = "tests/open.rs::opens";

/// The code store's view of a tree where two product types share the method leaf `new`, every
/// call of it qualified and made from a file holding no definition of `new`:
///
///  - `src/code_view.rs` holds the type `FactsView` and its method `new`;
///  - `src/snapshot.rs` holds the type `Snapshot` and its method `new`;
///  - `src/composed.rs` holds the type `Composed`, whose method `open` calls `FactsView::new`;
///  - `src/report.rs` holds `summary`, calling `FactsView.new`;
///  - `src/audit.rs` holds `audit`, calling `Snapshot::new`;
///  - `tests/open.rs` holds the test `opens`, calling `Snapshot::new`.
fn qualified_callers_in_other_files() -> FactsView {
    let member = |path: &str, parent: &str, leaf: &str| {
        let mut file = planted_file(path, &[(parent, CODE_KIND_TYPE)], &[], &[]);
        define_member(&mut file, parent, leaf, CODE_KIND_METHOD, 2);
        file
    };
    let caller = |path: &str, function: &str, name: &str| {
        let file = planted_file(path, &[(function, CODE_KIND_FUNCTION)], &[], &[]);
        with_calls(file, &[(function, name, "new")])
    };
    let composed = member("src/composed.rs", "Composed", "open");
    let composed = with_calls(composed, &[("open", "FactsView::new", "new")]);
    facts_view(vec![
        member("src/code_view.rs", "FactsView", "new"),
        member("src/snapshot.rs", "Snapshot", "new"),
        composed,
        caller("src/report.rs", "summary", "FactsView.new"),
        caller("src/audit.rs", "audit", "Snapshot::new"),
        caller("tests/open.rs", "opens", "Snapshot::new"),
    ])
}

/// NEAR FOLDS QUALIFIED CALLERS: each row is what the answer shows, the tests filter, the seed,
/// and its callers in id order, each with the tier of its `CALLS` edge into the seed in
/// `--around`'s neighbourhood ([`around`], two hops, as `rigger graph --around` reads it). Over
/// each half, the neighbourhood draws exactly those `CALLS` edges into the seed and counts no
/// hidden test hit, and the callers walk up from the seed reaches exactly the seed and those
/// callers, each by one edge and under no frontier.
#[test]
fn around_a_qualified_calls_one_candidate_lists_its_callers_in_other_files_over_both_halves() {
    let view = qualified_callers_in_other_files();
    let records = Indexed::default();
    let extracted = |from: &'static str| (from, TIER_EXTRACTED);
    let cases = [
        (
            "the path-qualified and the member-qualified caller in other files",
            Tests::Exclude,
            NEW,
            vec![extracted(OPEN), extracted(SUMMARY)],
        ),
        (
            "the same callers, tests shown, the test calling Snapshot::new not among them",
            Tests::Include,
            NEW,
            vec![extracted(OPEN), extracted(SUMMARY)],
        ),
        (
            "the other definition's product and test callers, tests shown",
            Tests::Include,
            SNAPSHOT_NEW,
            vec![extracted(AUDIT), extracted(OPENS)],
        ),
        (
            "the other definition's test caller, tests alone",
            Tests::Only,
            SNAPSHOT_NEW,
            vec![extracted(OPENS)],
        ),
    ];
    for (shows, tests, seed, callers) in cases {
        let want: Vec<(String, String)> = (callers.iter())
            .map(|(from, tier)| (from.to_string(), tier.to_string()))
            .collect();
        let mut reached: Vec<String> = callers.iter().map(|(from, _)| from.to_string()).collect();
        reached.push(seed.to_string());
        reached.sort();
        let walked: Vec<(String, String)> = (callers.iter())
            .map(|(from, _)| (from.to_string(), seed.to_string()))
            .collect();
        for (half, graph) in halves(&records, &view, tests) {
            let at = format!("{half} {tests:?} {seed}: {shows}");
            let drawn = around(&*graph, &[seed.to_string()], 2).unwrap();
            let mut into: Vec<(String, String)> = (drawn.graph.edges.iter())
                .filter(|e| e.rel == REL_CALLS && e.to == seed)
                .map(|e| (e.from.clone(), e.tier.to_string()))
                .collect();
            into.sort();
            assert_eq!((into, drawn.tests_hidden), (want.clone(), 0), "{at}");
            let up = call_walk(&*graph, seed, Direction::Up);
            assert_eq!(
                (
                    call_node_ids(&up),
                    call_frontiers(&up),
                    call_edge_pairs(&up)
                ),
                (reached.clone(), vec![], walked.clone()),
                "{at}"
            );
        }
    }
}
