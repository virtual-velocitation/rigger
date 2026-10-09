//! Periphery (CLI / end-to-end) tests for spec 53's DETECTION PASS (criterion 1) exercised through
//! the BUILT BINARY: `rigger graph communities [--resolution <r>]`. The inside-out algorithm unit
//! tests feed a hand-built `Coupling`, and the sibling library-level periphery
//! (`community_detection_pass.rs`) drives `Coupling::from_graph` / `detect` / `events` over a real
//! `whole()` read but MANUALLY applies the resulting events. So neither exercises the actual
//! subcommand: its dispatch, its argument parsing, its store bootstrap, or the real
//! `FoldingStore::append_and_fold` seam that appends the `CommunityAssigned` events to the run log AND folds
//! them into live `IN_COMMUNITY` edges under the local `.rigger/`. This file guards exactly that
//! binary boundary, over the shipped executable and the public projection surface:
//!
//!  - SUBCOMMAND wiring + arg parsing: `graph communities` dispatches, `--resolution` parses, and a
//!    malformed or unknown argument exits non-zero with its documented message (before any store
//!    side effect).
//!  - the END-TO-END record seam over the REAL store: a seeded coupling graph, detected through the
//!    binary, materializes a live community layer (`KIND_COMMUNITY` nodes + `IN_COMMUNITY` edges)
//!    read back over `Projector::whole`, and the summary line reports honest counts.
//!  - the EMPTY no-op: a project with no coupling edges records nothing and still exits 0.
//!  - GRAIN coexistence + supersession: two resolutions coexist (distinct community ids, both live),
//!    and re-running one grain REPLACES only that grain's assignment set - no stale duplicate
//!    memberships and the other grain untouched.
//!
//! The seed is built from the ALWAYS-COMPILED entity / call folds (spec 29a / 37), and detection,
//! the fold, and `FoldingStore::append_and_fold` are all always-compiled, so every test here runs
//! identically in BOTH feature lanes.

use std::path::Path;

use rigger::conductor;
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_COMMUNITY, REL_IN_COMMUNITY, TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, ExpectedRevision};
use rigger::ingest::folding_into;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::fixtures::{call_json, code_entity_json, event_of};
#[path = "common/layer_cli.rs"]
mod layer_cli;
use layer_cli::{member_of, rigger_db, LayerCli};

/// A stable project identity pinned into the fixture, so the in-test SEED store and the BINARY
/// resolve the SAME namespace: the binary reads `.rigger/project.id` at the git top-level, and the
/// seed opens its `Store` / `Projector` under that identity, so a fold the seed lands is the exact
/// projection the binary later reads and records into.
const IDENTITY: &str = "commtest";

/// A `CodeEntityExtracted` event (spec 29a): one real definition, folding its file node, the
/// `<file>::<name>` entity node (its `name` attr marks it a real definition - the canonicalization
/// target), and their `CONTAINS` edge. The coupling structure detection runs over.
fn def(file: &str, name: &str) -> Event {
    event_of(
        TYPE_CODE_ENTITY_EXTRACTED,
        code_entity_json(file, name, "function", 1, "rust"),
    )
}

/// A caller-attributed `EdgeInferred` call (spec 37): `<file>::<caller> --CALLS--> <name>`, landing
/// on a BARE same-file placeholder the detection pass resolves by unique name-suffix when `name` is
/// defined in ANOTHER file - the cross-directory coupling detection spans.
fn call(file: &str, name: &str, caller: &str) -> Event {
    event_of(TYPE_EDGE_INFERRED, call_json(file, name, caller))
}

/// The subcommand under test: `rigger graph communities`, its live layer the `KIND_COMMUNITY`
/// nodes and `IN_COMMUNITY` edges, seeded from a two-subsystem coupling graph.
const COMMUNITIES: LayerCli = LayerCli {
    subcommand: "communities",
    identity: IDENTITY,
    kind: KIND_COMMUNITY,
    rel: REL_IN_COMMUNITY,
    id_prefix: "community",
    empty_summary: [
        "detected 0 communities",
        "0 coupled node(s)",
        "0 membership event(s) recorded",
    ],
    seed: seed_coupling,
};

/// Seed a TWO-SUBSYSTEM coupling graph into the fixture's REAL store via the exact production seam
/// (`FoldingStore::append_and_fold` on the run stream): it appends the entity / call events to
/// `.rigger/events.db` and folds them into `.rigger/graph.db`. Subsystem A spans `src/combat` and
/// `src/net`; subsystem B spans `src/render` and `src/ui`; a single thin bridge joins them. Every
/// cross-file call lands on a bare placeholder the pass resolves, so A's entities couple across the
/// combat/net line and B's across the render/ui line. The store / projection handles drop at
/// function end, freeing the sqlite files before the binary opens them.
fn seed_coupling(root: &Path) {
    // The layer is seeded from derived events, which a store refuses: they land as the rows of
    // a store recorded before the ledger.
    let db = rigger_db(root, "events.db");
    let backend = Store::open(&db).unwrap();
    let pre_ledger = common::fixtures::PreLedgerStore {
        db: Path::new(&db),
        inner: &backend,
    };
    let store = Namespaced::new(&pre_ledger, IDENTITY);
    let graph = Projector::open(&rigger_db(root, "graph.db"), IDENTITY).unwrap();

    let events = vec![
        // Definitions.
        def("src/combat/hit.rs", "strike"),
        def("src/combat/hit.rs", "block"),
        def("src/net/link.rs", "send"),
        def("src/net/link.rs", "recv"),
        def("src/render/draw.rs", "paint"),
        def("src/render/draw.rs", "shade"),
        def("src/ui/hud.rs", "layout"),
        def("src/ui/hud.rs", "show"),
        // Subsystem A: dense cross-file coupling combat <-> net.
        call("src/combat/hit.rs", "send", "strike"),
        call("src/combat/hit.rs", "recv", "strike"),
        call("src/combat/hit.rs", "send", "block"),
        call("src/net/link.rs", "strike", "send"),
        call("src/net/link.rs", "block", "recv"),
        // Subsystem B: dense cross-file coupling render <-> ui.
        call("src/render/draw.rs", "layout", "paint"),
        call("src/render/draw.rs", "show", "paint"),
        call("src/render/draw.rs", "layout", "shade"),
        call("src/ui/hud.rs", "paint", "layout"),
        call("src/ui/hud.rs", "shade", "show"),
        // A single weak bridge A -> B (one call): too thin to fuse the subsystems.
        call("src/combat/hit.rs", "paint", "strike"),
    ];
    let done = folding_into(&store, Some(&graph as &dyn Projection), &|_| {})
        .append_and_fold(conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed the coupling graph through the real append-and-fold seam");
    assert_eq!(done.fold, rigger::contextgraph::Fold::Folded);
}

#[test]
fn the_subcommand_records_a_live_community_layer_over_the_real_store() {
    // Drive the built binary end-to-end: it reads the seeded coupling graph via `whole()`, detects
    // communities, and records them THROUGH `FoldingStore::append_and_fold` into live `IN_COMMUNITY` edges -
    // the seam the library-level periphery (which hand-applies events) never exercises.
    let dir = COMMUNITIES.project();
    let root = dir.path();
    seed_coupling(root);

    let out = COMMUNITIES.run(root, &[]);
    assert!(
        out.status.success(),
        "graph communities must succeed over a real seeded store: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("graph communities: detected"),
        "the summary line reports the detection outcome: {stdout}"
    );
    assert!(
        stdout.contains("at resolution 1"),
        "the summary reports the default resolution grain: {stdout}"
    );
    assert!(
        stdout.contains("membership event(s) recorded"),
        "the summary reports the events recorded into the store: {stdout}"
    );

    let (comms, edges) = COMMUNITIES.live_layer(root);
    assert!(
        comms.len() >= 2,
        "at least the two subsystems became live communities, got {}: {comms:?}",
        comms.len()
    );
    for c in &comms {
        assert!(
            c.starts_with("community/1/"),
            "the default-grain pass names communities `community/1/<n>`: {c}"
        );
    }

    // Subsystem A: `strike` (combat) and `send` (net) couple into ONE community across the directory
    // line - the bare cross-file placeholder resolved onto the real `send` definition.
    let a = member_of(&edges, "src/combat/hit.rs::strike")
        .expect("the combat definition is a live community member");
    assert_eq!(
        member_of(&edges, "src/net/link.rs::send"),
        Some(a),
        "strike (combat) and send (net) group into one community across directory lines"
    );
    // Subsystem B: `paint` (render) and `layout` (ui) couple into a DIFFERENT community.
    let b = member_of(&edges, "src/render/draw.rs::paint")
        .expect("the render definition is a live community member");
    assert_eq!(
        member_of(&edges, "src/ui/hud.rs::layout"),
        Some(b),
        "paint (render) and layout (ui) group into one community across directory lines"
    );
    assert_ne!(
        a, b,
        "the thin bridge does not fuse the two subsystems into one community"
    );

    // The bare cross-file placeholders resolved away: only real definitions / files are members.
    assert!(
        member_of(&edges, "src/combat/hit.rs::send").is_none(),
        "a bare cross-file placeholder is never a live member on its own"
    );
}

rigger::test_cases! {
    an_empty_project_records_no_community_and_still_succeeds:
        COMMUNITIES.an_empty_project_records_nothing_and_still_succeeds();
    resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain:
        COMMUNITIES.resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain();
    a_malformed_resolution_or_unknown_argument_fails_loudly:
        COMMUNITIES.a_malformed_resolution_or_unknown_argument_fails_loudly();
    re_running_a_grain_reproduces_the_byte_identical_live_layer:
        COMMUNITIES.re_running_a_grain_reproduces_the_byte_identical_live_layer();
    a_pass_whose_fold_is_lost_to_a_lock_says_so_and_the_next_pass_refuses:
        COMMUNITIES.a_pass_whose_fold_is_lost_to_a_lock_says_so_and_the_next_pass_refuses();
}
