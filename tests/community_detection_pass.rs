//! Periphery (end-to-end) test for spec 53's DETERMINISTIC DETECTION (criterion 1): the offline
//! `rigger::community` pass reads a REAL folded coupling graph, groups densely-coupled entities
//! ACROSS directory lines, and a rebuild from the recorded events reproduces byte-identical
//! membership rows. It runs over the library's PUBLIC surface (`Projector` + `rigger::community`),
//! so it guards the read -> detect -> record -> fold -> rebuild seam the in-crate algorithm unit
//! tests (which feed a hand-built graph) are structurally blind to:
//!
//!  - the CROSS-FILE resolution over a real `whole()` read. A reference in one file to a symbol
//!    defined in another folds onto a BARE same-file placeholder (spec 29a keeps cross-file name
//!    resolution out of the fold); detection must redirect that placeholder onto the unique real
//!    definition, so a community spans the directory boundary between caller and callee. The unit
//!    tests hand-build already-resolved edges and never exercise this over the projection.
//!  - the REBUILD-BYTE-IDENTICAL contract through the ACTUAL fold. The pass's `CommunityAssigned`
//!    events, replayed from scratch into a fresh projection, must reproduce the same community nodes
//!    (including the degree-derived label) and the same live `IN_COMMUNITY` edges - the rebuildable
//!    projection invariant the criterion names.
//!
//! Detection and the fold are ALWAYS compiled, so this is not feature-gated and runs in BOTH lanes.

use std::collections::BTreeMap;

use rigger::community::{self, Coupling, DEFAULT_RESOLUTION};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Graph, Projection, KIND_COMMUNITY, REL_IN_COMMUNITY};
use rigger::eventstore::Event;

#[path = "common/graph_fold.rs"]
mod graph_fold;
use graph_fold::seed_two_subsystems;

/// A deterministic snapshot of the whole community layer read over the PUBLIC surface: every
/// `KIND_COMMUNITY` node (id, kind, ordered attrs) and every LIVE `IN_COMMUNITY` edge (from, to),
/// sorted. Two derivations of the same assignment set must produce byte-identical snapshots.
fn community_snapshot(g: &Graph) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for n in &g.nodes {
        if n.kind == KIND_COMMUNITY {
            let attrs: BTreeMap<&String, &String> = n.attrs.iter().collect();
            rows.push(format!("node {}|{}|{attrs:?}", n.id, n.kind));
        }
    }
    for e in &g.edges {
        if e.rel == REL_IN_COMMUNITY {
            rows.push(format!("edge {} -{}-> {}", e.from, e.rel, e.to));
        }
    }
    rows.sort();
    rows
}

/// Run the pass on a projector seeded with the coupling graph, returning (assignment, the recorded
/// community events positioned onto the log after the seed).
fn run_pass(p: &Projector, next_pos: u64) -> (community::Assignment, Vec<Event>) {
    let whole = p.whole().unwrap();
    let coupling = Coupling::from_graph(&whole);
    let assignment = community::detect(&coupling, DEFAULT_RESOLUTION);
    let mut events = community::events(&assignment);
    for (i, e) in events.iter_mut().enumerate() {
        e.position = next_pos + i as u64;
    }
    (assignment, events)
}

#[test]
fn the_pass_groups_densely_coupled_entities_across_directory_lines() {
    // Detection over a REAL folded coupling graph groups each subsystem's entities into ONE
    // community REGARDLESS of directory (combat+net, render+ui), and the thin bridge does not fuse
    // the two subsystems.
    let p = Projector::open(":memory:", "test").unwrap();
    let next = seed_two_subsystems(&p);
    let (assignment, _events) = run_pass(&p, next);
    let m: BTreeMap<String, String> = assignment.members.iter().cloned().collect();

    // The bare cross-file placeholders resolved away - only real definitions and files are members.
    assert!(
        !m.contains_key("src/combat/hit.rs::send"),
        "a bare cross-file placeholder is never a member on its own"
    );

    // Subsystem A: `strike` (combat) and `send` (net) share one community across the directory line.
    let a = &m["src/combat/hit.rs::strike"];
    assert_eq!(
        &m["src/net/link.rs::send"], a,
        "strike (combat) and send (net) group into one community across directory lines"
    );
    // Subsystem B: `paint` (render) and `layout` (ui) share a DIFFERENT community.
    let b = &m["src/render/draw.rs::paint"];
    assert_eq!(
        &m["src/ui/hud.rs::layout"], b,
        "paint (render) and layout (ui) group into one community across directory lines"
    );
    assert_ne!(a, b, "the thin bridge does not merge the two subsystems");
    assert!(
        assignment.num_communities >= 2,
        "at least the two subsystems, got {}",
        assignment.num_communities
    );
}

#[test]
fn a_rebuild_from_the_recorded_events_reproduces_byte_identical_membership_rows() {
    // The rebuildable-projection invariant criterion 1 names: the pass records `CommunityAssigned`
    // events; replaying the WHOLE log (coupling seed + those events) from scratch into a fresh
    // projection reproduces byte-identical community nodes (label included) and `IN_COMMUNITY` edges.
    let p1 = Projector::open(":memory:", "test").unwrap();
    let next = seed_two_subsystems(&p1);
    let (_assignment, events) = run_pass(&p1, next);
    // Materialize the community layer on the first projection.
    for e in &events {
        p1.apply(e).unwrap();
    }
    let snapshot_a = community_snapshot(&p1.whole().unwrap());

    // Rebuild: a fresh projection replays the SAME seed and the SAME recorded events.
    let p2 = Projector::open(":memory:", "test").unwrap();
    seed_two_subsystems(&p2);
    for e in &events {
        p2.apply(e).unwrap();
    }
    let snapshot_b = community_snapshot(&p2.whole().unwrap());

    assert!(
        !snapshot_a.is_empty(),
        "the pass recorded a non-empty community layer"
    );
    assert_eq!(
        snapshot_a, snapshot_b,
        "a rebuild from the recorded events reproduces byte-identical community rows"
    );
}

#[test]
fn two_runs_record_byte_identical_events() {
    // Determinism at the recorded-event level: two independent runs over the same coupling graph
    // record byte-identical `CommunityAssigned` payloads (the bytes a rebuild replays), so the
    // assignment is reproducible run-to-run, not merely within one process.
    let p1 = Projector::open(":memory:", "test").unwrap();
    let next1 = seed_two_subsystems(&p1);
    let (_a1, e1) = run_pass(&p1, next1);

    let p2 = Projector::open(":memory:", "test").unwrap();
    let next2 = seed_two_subsystems(&p2);
    let (_a2, e2) = run_pass(&p2, next2);

    let wire = |events: &[Event]| -> Vec<(String, Vec<u8>)> {
        events
            .iter()
            .map(|e| (e.type_.clone(), e.data.clone()))
            .collect()
    };
    assert_eq!(
        wire(&e1),
        wire(&e2),
        "two runs record byte-identical community-assignment events"
    );
}
