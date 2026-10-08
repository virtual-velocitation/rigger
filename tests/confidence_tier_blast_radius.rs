//! Periphery (integration) tests for spec 29c criterion 2: the CONFIDENCE-TIER blast radius. These
//! run OUTSIDE the crate, over the library's PUBLIC surface, so they guard the boundary the
//! inside-out unit tests are structurally blind to.
//!
//! Criterion 2's whole change lives in `grounded_blast_radius` / `confidence_tier_radius` /
//! `files_reachable` - all PRIVATE to `conductor`. The inside-out unit tests reach those private
//! functions directly (a hand-built `RunCtx` / a direct call) and assert on the returned
//! `BlastRadius` STRUCT. Nothing pins the behavior at the PUBLIC edge where it actually matters: a
//! real run computes the radius over an INJECTED graph and RECORDS it as a serialized
//! `BlastRadiusComputed` audit event - the observable artifact the runtime parallelism-retention
//! metric and the operator read back. The load-bearing invariant that must survive that record path,
//! on the branch where criterion 2's new code runs (a graph is present), is that the grounder's own
//! radius survives into the recorded safe view - the tier-filter arm UNIONS it in, so a file only
//! the grounder reaches is never dropped (addendum 2.4 states the safe view's contract).
//!
//! These tests inject a STRUCTURAL grounder double (a non-empty `index_stamp`, so the audit is
//! emitted at all), populate the unified graph through the public `contextgraph` event API (the
//! same serialized events a real run folds), drive the public `conductor::run` entry, and assert
//! on the recorded `BlastRadiusComputed` event. They exercise the new cross-module seam end-to-end:
//! `grounded_blast_radius` reading the injected `graph` port, tier-filtering the ONE subgraph, and
//! the two-view radius reaching the serialized audit.
//!
//! The tier filter and the audit fields are always compiled, so these guard the boundary in BOTH
//! feature lanes.

use rigger::conductor::{run, Deps, STREAM, TYPE_BLAST_RADIUS_COMPUTED};
use rigger::config::{AgentDef, Config, Stage};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::gate::ExecRunner;
use rigger::grounder::{BlastRadius, Grounder, Ref};
use serde_json::{json, Value};

mod common;
use common::fixtures::apply_next_json;
use common::fixtures::NoopDriver;

/// A STRUCTURAL grounder double - the shape criterion 2's graph arm and the audit actually key off,
/// which a grep grounder cannot stand in for:
///
///   - it seeds the traversal on `combat.rs` (so the graph `subgraph(seed, 2)` reads the tiered
///     neighborhood populated below);
///   - its `blast_radius` is the GREP FLOOR the tier-filter arm must union into `safe`: it carries a
///     `grep_only.rs` file the graph does not;
///   - its `index_stamp` is NON-EMPTY: that is the structural-active signal `record_blast_radius`
///     keys the `BlastRadiusComputed` audit off, so the radius is actually serialized to an event.
struct StampedGrounder;

const GREP_ONLY_FILE: &str = "grep_only.rs";

impl Grounder for StampedGrounder {
    fn ground(&self, _query: &str, _k: usize) -> Vec<Ref> {
        vec![Ref {
            file: "combat.rs".into(),
            line: 0,
            text: String::new(),
        }]
    }

    fn blast_radius(&self, _query: &str, _k: usize) -> BlastRadius {
        // The grep floor: the seed file plus a file the graph traversal never reaches. The tier
        // filter must UNION this in, so `grep_only.rs` survives into the recorded `safe`.
        BlastRadius {
            precise: vec!["combat.rs".into(), GREP_ONLY_FILE.into()],
            safe: vec!["combat.rs".into(), GREP_ONLY_FILE.into()],
        }
    }

    fn index_stamp(&self) -> String {
        // Non-empty: the structural-active signal that makes the conductor emit the audit at all.
        "test-index-stamp/v1".into()
    }
}

/// A real projector populated with a production-faithful multi-tier neighborhood of the seed file
/// `combat.rs`, folded through the public event API (`CodeEntityExtracted` / `EdgeInferred`) exactly
/// as a live run would: `combat.rs` DEFINES `apply_damage` and references it same-file (EXTRACTED),
/// references `shared` which is defined in `util.rs` (INFERRED), and references `magic` which is
/// defined nowhere (AMBIGUOUS). So the seed's subgraph carries all three confidence tiers - the tier
/// filter runs over a real edge set, not a hand-built fixture.
fn tiered_projector() -> Projector {
    let g = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;
    // combat.rs defines apply_damage (EXTRACTED CONTAINS) ...
    apply_next_json(
        &g,
        &mut pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        json!({ "file": "combat.rs", "name": "apply_damage", "kind": "function", "line": 1, "lang": "rust" }),
    );
    // ... and util.rs defines shared, so combat.rs's reference to it resolves cross-file (INFERRED).
    apply_next_json(
        &g,
        &mut pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        json!({ "file": "util.rs", "name": "shared", "kind": "function", "line": 1, "lang": "rust" }),
    );
    // combat.rs references apply_damage (same-file: EXTRACTED), shared (cross-file: INFERRED), and
    // magic (defined nowhere: AMBIGUOUS) - one reference per confidence tier.
    for name in ["apply_damage", "shared", "magic"] {
        apply_next_json(
            &g,
            &mut pos,
            TYPE_EDGE_INFERRED,
            json!({ "file": "combat.rs", "name": name, "lang": "rust" }),
        );
    }
    g
}

/// Drive `conductor::run` over a single stage whose grounding seeds on `combat.rs`, with the
/// structural grounder double and (optionally) the tiered graph injected, then return the parsed
/// payload of the `BlastRadiusComputed` audit event the run recorded. `None` means no audit was
/// emitted.
fn recorded_blast_radius(graph: Option<&Projector>) -> Option<Value> {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "impl".into(),
        AgentDef {
            id: "impl".into(),
            ..Default::default()
        },
    );
    cfg.workflow.stages.insert(
        "s".into(),
        Stage {
            name: "s".into(),
            agent: "impl".into(),
            coverage: "combat".into(),
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    let driver = NoopDriver;
    let grounder = StampedGrounder;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: Some(&grounder),
        graph: graph.map(|g| g as _),
        criteria: Vec::new(),
        log: &|_| {},
    };
    // The radius is recorded before the spawn, so the run's terminal disposition is irrelevant.
    let _ = run(&cfg, &deps);

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    events
        .iter()
        .find(|e| e.type_ == TYPE_BLAST_RADIUS_COMPUTED)
        .map(|e| serde_json::from_slice(&e.data).unwrap())
}

fn safe_of(payload: &Value) -> Vec<String> {
    payload["safe"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

/// The load-bearing correctness invariant of criterion 2 (section 2.4), pinned at the PUBLIC
/// serialized boundary: with a graph injected - the branch where the tier-filter arm runs - the
/// grounder's radius survives into the recorded `BlastRadiusComputed.safe`. The grounder's radius
/// carries `grep_only.rs`, a file the graph traversal never reaches; the tier-filter arm UNIONS the
/// grounder's radius in, so `grep_only.rs` must survive into the recorded `safe` (addendum 2.4).
///
/// Non-vacuous: if the graph arm returned only the tier-filter's `safe` (dropping the grounder's
/// radius), `grep_only.rs` would vanish from the recorded event and this assertion would fail - so
/// it guards exactly the "the grounder's radius survives" invariant against a real record path.
#[test]
fn the_graph_path_records_a_safe_radius_that_stays_a_grep_superset() {
    let graph = tiered_projector();
    let payload = recorded_blast_radius(Some(&graph))
        .expect("a BlastRadiusComputed audit IS emitted under the structural grounder");
    let safe = safe_of(&payload);

    // The grep floor's files both survive into the recorded safe (the union is preserved) ...
    assert!(
        safe.contains(&"combat.rs".to_string()),
        "the seed file must be in the recorded safe radius; got {safe:?}"
    );
    assert!(
        safe.contains(&GREP_ONLY_FILE.to_string()),
        "the tier-filter arm must UNION the grounder's radius, so its file survives into the \
         recorded safe (addendum 2.4); got {safe:?}"
    );
}

/// The `graph: None` fallback, pinned at the same serialized boundary: with no graph the recorded
/// radius is the grounder's radius verbatim - the tier-filter arm is skipped and the grep floor
/// (including `grep_only.rs`) passes through unchanged. This is the precondition that makes the
/// graph-path test above meaningful: the graph arm is what runs the new code, and it must preserve
/// everything the fallback preserves (it may only WIDEN `safe`, never narrow it).
#[test]
fn the_no_graph_fallback_records_the_grounder_radius_verbatim() {
    let payload = recorded_blast_radius(None)
        .expect("a BlastRadiusComputed audit IS emitted under the structural grounder");
    let safe = safe_of(&payload);

    assert!(
        safe.contains(&"combat.rs".to_string()) && safe.contains(&GREP_ONLY_FILE.to_string()),
        "the no-graph fallback records the grounder's grep radius verbatim; got {safe:?}"
    );
}
