//! Periphery (contract / API / integration) tests for spec 107 criterion 1, THE CLASSES ARE ONE
//! TABLE. These run OUTSIDE the crates, over the root crate's public surface, and guard three
//! boundaries the class-table scan and the in-crate fold test are structurally blind to.
//!
//! The scan (`tests/event_classes_are_one_table.rs`) reads a `TYPE_` alias as declaring no type,
//! so it cannot see WHICH constant a re-export names: a conductor re-export pointed at the wrong
//! ledger constant passes it. The first test pins every moved type string through both public
//! paths a caller spells, `rigger::conductor::TYPE_*` and `rigger::ledger::TYPE_*`.
//!
//! `retention` cites the derived index of `ingest`, so it must sit under `ingest`'s feature gate in
//! both crates that declare it. The second test pins the gate line itself, in the domain crate and
//! at the root re-export.
//!
//! The in-crate fold test reads the projector's private tables. The third test folds one event of
//! each episodic type through the public fold and compares the projector's public whole-graph
//! read, every live node and edge, then folds a decision after them as the positive control: the
//! read that stayed still moves when a knowledge event arrives, at that event's own position.

mod common;

use common::cli::nanos;
use common::fixtures::{apply_governs_at, apply_json, governs, live_node_ids, seed_two_subsystems};
use common::repo::repo_text;
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Graph, KIND_DECISION, KIND_FILE};
use rigger::retention::EPISODIC_TYPES;

/// The feature gate `ingest` and `retention` are declared under.
const STORE_GATE: &str = "#[cfg(any(feature = \"store\", not(feature = \"core\")))]";

/// The four files `seed_two_subsystems` defines entities in.
const SEEDED_FILES: [&str; 4] = [
    "src/combat/hit.rs",
    "src/net/link.rs",
    "src/render/draw.rs",
    "src/ui/hud.rs",
];

#[test]
fn each_moved_type_string_reads_the_same_through_the_conductor_and_the_ledger() {
    for (name, through_conductor, through_ledger, wire) in [
        (
            "TYPE_GATE_PROMOTED",
            rigger::conductor::TYPE_GATE_PROMOTED,
            rigger::ledger::TYPE_GATE_PROMOTED,
            "GatePromoted",
        ),
        (
            "TYPE_GATE_DEMOTED",
            rigger::conductor::TYPE_GATE_DEMOTED,
            rigger::ledger::TYPE_GATE_DEMOTED,
            "GateDemoted",
        ),
        (
            "TYPE_SCOPE_CREEP",
            rigger::conductor::TYPE_SCOPE_CREEP,
            rigger::ledger::TYPE_SCOPE_CREEP,
            "ScopeCreep",
        ),
        (
            "TYPE_SPEC_DEFECT",
            rigger::conductor::TYPE_SPEC_DEFECT,
            rigger::ledger::TYPE_SPEC_DEFECT,
            "SpecDefect",
        ),
        (
            "TYPE_TASK_ABORTED",
            rigger::conductor::TYPE_TASK_ABORTED,
            rigger::ledger::TYPE_TASK_ABORTED,
            "TaskAborted",
        ),
    ] {
        assert_eq!(
            through_conductor, wire,
            "conductor::{name} keeps its wire string"
        );
        assert_eq!(through_ledger, wire, "ledger::{name} is the wire string");
    }
}

/// The trimmed line directly above the one line of `rel` that is exactly `declaration`.
fn line_above(rel: &str, declaration: &str) -> String {
    let text = repo_text(rel);
    let lines: Vec<&str> = text.lines().collect();
    let at: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == declaration)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(at.len(), 1, "{rel} declares `{declaration}` exactly once");
    lines[at[0] - 1].trim().to_string()
}

#[test]
fn retention_is_declared_under_the_feature_gate_ingest_carries() {
    for (rel, declaration) in [
        ("crates/rigger-domain/src/lib.rs", "pub mod ingest;"),
        ("crates/rigger-domain/src/lib.rs", "pub mod retention;"),
        ("src/lib.rs", "pub use rigger_domain::retention;"),
    ] {
        assert_eq!(
            line_above(rel, declaration),
            STORE_GATE,
            "{rel}: `{declaration}` sits directly under the store gate"
        );
    }
}

/// The projector's public read of the whole live graph.
fn served(p: &Projector) -> Graph {
    p.whole().unwrap()
}

/// Every node and edge of `g` as one row each, sorted, so two reads compare whatever order the
/// projection hands them in.
fn rows(g: &Graph) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for n in &g.nodes {
        rows.push(format!("node\t{}\t{}\t{:?}", n.id, n.kind, n.attrs));
    }
    for e in &g.edges {
        rows.push(format!(
            "edge\t{}\t{}\t{}\t{}\t{:?}\t{}\t{}",
            e.from, e.rel, e.to, e.valid_from, e.valid_to, e.source, e.tier
        ));
    }
    rows.sort();
    rows
}

#[test]
fn folding_every_episodic_type_leaves_the_served_graph_as_it_stood_and_a_decision_after_them_lands()
{
    // Given a graph holding code structure and one decision.
    let p = Projector::open(":memory:", "test").unwrap();
    let seeded_to = seed_two_subsystems(&p);
    assert_eq!(seeded_to, 20, "the seed leaves position 20 free");
    apply_governs_at(&p, 20, "d1", "src/combat/hit.rs", 100);
    let before = served(&p);
    let before_rows = rows(&before);
    assert_eq!(
        live_node_ids(&before, KIND_FILE),
        SEEDED_FILES
            .iter()
            .map(|f| (*f).to_string())
            .collect::<Vec<String>>(),
        "the served graph holds the four seeded files, so an unchanged read proves something"
    );
    assert_eq!(
        governs(&before.edges),
        vec![(
            "d1".to_string(),
            "src/combat/hit.rs".to_string(),
            20,
            nanos(100)
        )],
        "the served graph holds the seeded decision's edge"
    );
    assert_eq!(
        live_node_ids(&before, KIND_DECISION),
        vec!["d1".to_string()],
        "the served graph holds the seeded decision"
    );

    // When one event of each episodic type is folded, its payload naming a seeded file.
    let mut pos = 20;
    for type_ in EPISODIC_TYPES {
        pos += 1;
        apply_json(
            &p,
            pos,
            type_,
            serde_json::json!({
                "id": "e1", "unit": "u1", "by": "rust-engineer", "path": "src/combat/hit.rs",
                "gate": "cargo test", "pass": true, "artifact": "src/combat/hit.rs",
            }),
        );
        // Then the served graph is row for row what it was.
        assert_eq!(
            rows(&served(&p)),
            before_rows,
            "folding {type_} changes nothing the projection serves"
        );
    }
    assert_eq!(pos, 36, "sixteen episodic types were folded");

    // And a decision folded after them lands at its own position, beside everything seeded.
    apply_governs_at(&p, 37, "d2", "src/net/link.rs", 500);
    let after = served(&p);
    let after_rows = rows(&after);
    assert_eq!(
        governs(&after.edges),
        vec![
            (
                "d1".to_string(),
                "src/combat/hit.rs".to_string(),
                20,
                nanos(100)
            ),
            (
                "d2".to_string(),
                "src/net/link.rs".to_string(),
                37,
                nanos(500)
            ),
        ],
        "the decision after the episodic events folds its edge at position 37"
    );
    assert_eq!(
        live_node_ids(&after, KIND_DECISION),
        vec!["d1".to_string(), "d2".to_string()],
        "the decision after the episodic events folds its node"
    );
    assert_eq!(
        (after.nodes.len(), after.edges.len()),
        (before.nodes.len() + 1, before.edges.len() + 1),
        "the decision's node and its one edge are all the served graph gained"
    );
    assert_eq!(
        before_rows
            .iter()
            .filter(|row| !after_rows.contains(row))
            .collect::<Vec<&String>>(),
        Vec::<&String>::new(),
        "every row served before the episodic events is still served"
    );
}
