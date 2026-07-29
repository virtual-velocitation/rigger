//! Periphery (CLI / end-to-end) tests for spec 53's DETECTION PASS (criterion 1) exercised through
//! the BUILT BINARY: `rigger graph communities [--resolution <r>]`. The inside-out algorithm unit
//! tests feed a hand-built `Coupling`, and the sibling library-level periphery
//! (`community_detection_pass.rs`) drives `Coupling::from_graph` / `detect` / `events` over a real
//! `whole()` read but MANUALLY applies the resulting events. So neither exercises the actual
//! subcommand: its dispatch, its argument parsing, its store bootstrap, or the real
//! `append_and_fold_batch` seam that appends the `CommunityAssigned` events to the run log AND folds
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
//! the fold, and `append_and_fold_batch` are all always-compiled, so every test here runs
//! identically in BOTH feature lanes.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use rigger::conductor;
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_COMMUNITY, REL_IN_COMMUNITY, TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::Event;
use rigger::ingest::append_and_fold_batch;

/// The compiled `rigger` binary under test (Cargo sets this for integration tests).
fn rigger_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rigger")
}

/// A stable project identity pinned into the fixture, so the in-test SEED store and the BINARY
/// resolve the SAME namespace: the binary reads `.rigger/project.id` at the git top-level, and the
/// seed opens its `Store` / `Projector` under that identity, so a fold the seed lands is the exact
/// projection the binary later reads and records into.
const IDENTITY: &str = "commtest";

/// A throwaway git project with `.rigger/project.id` pinned. Its own git repo makes the identity
/// resolution deterministic (the top-level is the fixture), and the pinned id file makes the seed
/// and the binary agree on the store namespace. Kept alive by the caller.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| {
        let _ = Command::new("git").args(args).current_dir(root).status();
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(root.join(".rigger").join("project.id"), IDENTITY).unwrap();
    dir
}

/// The local `.rigger/<name>` path under the fixture, as the binary's `db_path` resolves it.
fn rigger_db(root: &Path, name: &str) -> String {
    root.join(".rigger")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// Run `rigger graph communities <args>` in `root` over the built binary and return the output.
fn communities(root: &Path, args: &[&str]) -> std::process::Output {
    let mut argv = vec!["graph", "communities"];
    argv.extend_from_slice(args);
    Command::new(rigger_bin())
        .args(&argv)
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .output()
        .expect("spawn rigger graph communities")
}

/// A `CodeEntityExtracted` event (spec 29a): one real definition, folding its file node, the
/// `<file>::<name>` entity node (its `name` attr marks it a real definition - the canonicalization
/// target), and their `CONTAINS` edge. The coupling structure detection runs over.
fn def(file: &str, name: &str) -> Event {
    Event::new(
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&serde_json::json!({
            "file": file, "name": name, "kind": "function", "line": 1, "lang": "rust"
        }))
        .unwrap(),
    )
}

/// A caller-attributed `EdgeInferred` call (spec 37): `<file>::<caller> --CALLS--> <name>`, landing
/// on a BARE same-file placeholder the detection pass resolves by unique name-suffix when `name` is
/// defined in ANOTHER file - the cross-directory coupling detection spans.
fn call(file: &str, name: &str, caller: &str) -> Event {
    Event::new(
        TYPE_EDGE_INFERRED,
        serde_json::to_vec(&serde_json::json!({
            "file": file, "name": name, "caller": caller, "lang": "rust"
        }))
        .unwrap(),
    )
}

/// Seed a TWO-SUBSYSTEM coupling graph into the fixture's REAL store via the exact production seam
/// (`append_and_fold_batch` on the run stream): it appends the entity / call events to
/// `.rigger/events.db` and folds them into `.rigger/graph.db`. Subsystem A spans `src/combat` and
/// `src/net`; subsystem B spans `src/render` and `src/ui`; a single thin bridge joins them. Every
/// cross-file call lands on a bare placeholder the pass resolves, so A's entities couple across the
/// combat/net line and B's across the render/ui line. The store / projection handles drop at
/// function end, freeing the sqlite files before the binary opens them.
fn seed_coupling(root: &Path) {
    let backend = Store::open(&rigger_db(root, "events.db")).unwrap();
    let store = Namespaced::new(&backend, IDENTITY);
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
    append_and_fold_batch(
        &store,
        Some(&graph as &dyn Projection),
        conductor::STREAM,
        &events,
    )
    .expect("seed the coupling graph through the real append-and-fold seam");
}

/// The live community layer read back over the PUBLIC projection surface: the sorted set of
/// `KIND_COMMUNITY` node ids, and every live `<member> --IN_COMMUNITY--> <community>` edge as
/// `(member, community)` pairs.
fn community_layer(root: &Path) -> (Vec<String>, Vec<(String, String)>) {
    let graph = Projector::open(&rigger_db(root, "graph.db"), IDENTITY).unwrap();
    let whole = graph.whole().unwrap();
    let mut comms: Vec<String> = whole
        .nodes
        .iter()
        .filter(|n| n.kind == KIND_COMMUNITY)
        .map(|n| n.id.clone())
        .collect();
    comms.sort();
    comms.dedup();
    let mut edges: Vec<(String, String)> = whole
        .edges
        .iter()
        .filter(|e| e.rel == REL_IN_COMMUNITY)
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    edges.sort();
    (comms, edges)
}

/// The live community a member currently belongs to (its single `IN_COMMUNITY` target), if any.
fn member_of<'a>(edges: &'a [(String, String)], node: &str) -> Option<&'a str> {
    edges
        .iter()
        .find(|(from, _)| from == node)
        .map(|(_, to)| to.as_str())
}

#[test]
fn the_subcommand_records_a_live_community_layer_over_the_real_store() {
    // Drive the built binary end-to-end: it reads the seeded coupling graph via `whole()`, detects
    // communities, and records them THROUGH `append_and_fold_batch` into live `IN_COMMUNITY` edges -
    // the seam the library-level periphery (which hand-applies events) never exercises.
    let dir = project();
    let root = dir.path();
    seed_coupling(root);

    let out = communities(root, &[]);
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

    let (comms, edges) = community_layer(root);
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

#[test]
fn an_empty_project_records_no_community_and_still_succeeds() {
    // A project with no coupling edges is a clean no-op end-to-end: the pass detects nothing,
    // records nothing, and exits 0 (never an error). This proves the CLI wiring, the store
    // bootstrap, and the `is_empty` no-op path over the built binary in BOTH feature lanes.
    let dir = project();
    let root = dir.path();

    let out = communities(root, &[]);
    assert!(
        out.status.success(),
        "an empty coupling graph is a no-op, not an error: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("detected 0 communities"),
        "the summary reports zero communities: {stdout}"
    );
    assert!(
        stdout.contains("0 coupled node(s)"),
        "the summary reports zero coupled nodes: {stdout}"
    );
    assert!(
        stdout.contains("0 membership event(s) recorded"),
        "an empty pass records no membership events: {stdout}"
    );

    let (comms, edges) = community_layer(root);
    assert!(
        comms.is_empty() && edges.is_empty(),
        "no community layer materialized for an empty project: {} node(s), {} edge(s)",
        comms.len(),
        edges.len()
    );
}

#[test]
fn resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain() {
    // Two grains are recorded from the same coupling graph, then the default grain is re-run. The
    // `--resolution` grains coexist (distinct `community/<r>/*` ids, both live), and a re-run of one
    // grain REPLACES only that grain's assignment set (the `fresh` pass boundary) - the other grain
    // is untouched, and the re-run leaves exactly one live membership per member (no duplicates).
    let dir = project();
    let root = dir.path();
    seed_coupling(root);

    assert!(
        communities(root, &[]).status.success(),
        "recording the default grain succeeds"
    );
    assert!(
        communities(root, &["--resolution", "2"]).status.success(),
        "recording a second grain (r=2) succeeds"
    );

    let (comms, _edges) = community_layer(root);
    let grain1: BTreeSet<&String> = comms
        .iter()
        .filter(|c| c.starts_with("community/1/"))
        .collect();
    let grain2_before: BTreeSet<String> = comms
        .iter()
        .filter(|c| c.starts_with("community/2/"))
        .cloned()
        .collect();
    assert!(!grain1.is_empty(), "the default grain is live");
    assert!(
        !grain2_before.is_empty(),
        "the r=2 grain coexists with the default grain (distinct ids), not destroyed by it"
    );

    // Re-run the default grain: it must supersede ONLY `community/1/*`, leaving `community/2/*` whole.
    assert!(
        communities(root, &[]).status.success(),
        "re-running the default grain succeeds"
    );
    let (comms2, edges2) = community_layer(root);
    let grain2_after: BTreeSet<String> = comms2
        .iter()
        .filter(|c| c.starts_with("community/2/"))
        .cloned()
        .collect();
    assert_eq!(
        grain2_before, grain2_after,
        "re-running the default grain leaves the r=2 grain's communities intact"
    );

    // After the re-run each default-grain member carries exactly ONE live membership - the prior
    // pass's memberships were superseded, not left as stale duplicates.
    let mut default_members: Vec<&String> = edges2
        .iter()
        .filter(|(_, to)| to.starts_with("community/1/"))
        .map(|(from, _)| from)
        .collect();
    let total = default_members.len();
    default_members.sort();
    default_members.dedup();
    assert!(
        total > 0,
        "the default grain still has live memberships after the re-run"
    );
    assert_eq!(
        default_members.len(),
        total,
        "each member has exactly one live default-grain membership after the re-run (supersession, \
         no duplicates)"
    );
}

#[test]
fn a_malformed_resolution_or_unknown_argument_fails_loudly() {
    // Argument validation runs BEFORE any store side effect: a non-numeric resolution, a
    // non-positive resolution, and an unknown argument each exit non-zero with the documented
    // message. This pins the subcommand's parsing contract over the built binary.
    let dir = project();
    let root = dir.path();

    let non_number = communities(root, &["--resolution", "not-a-number"]);
    assert!(
        !non_number.status.success(),
        "a non-numeric --resolution is rejected"
    );
    assert!(
        String::from_utf8_lossy(&non_number.stderr).contains("--resolution expects a number"),
        "the error names the malformed numeric argument: {}",
        String::from_utf8_lossy(&non_number.stderr)
    );

    let non_positive = communities(root, &["--resolution", "0"]);
    assert!(
        !non_positive.status.success(),
        "a non-positive --resolution is rejected"
    );
    assert!(
        String::from_utf8_lossy(&non_positive.stderr).contains("positive finite number"),
        "the error demands a positive finite resolution: {}",
        String::from_utf8_lossy(&non_positive.stderr)
    );

    let unknown = communities(root, &["--bogus"]);
    assert!(!unknown.status.success(), "an unknown argument is rejected");
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("unknown argument"),
        "the error names the unknown argument: {}",
        String::from_utf8_lossy(&unknown.stderr)
    );
}
