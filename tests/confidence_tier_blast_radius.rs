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
//! metric and the operator read back. Two load-bearing invariants must survive that record path, on
//! the branch where criterion 2's new code runs (a graph is present):
//!
//!   1. the SAFE superset stays a superset of the grep union (section 2.4) - the tier-filter arm
//!      UNIONS the grounder's grep radius, so a grep-only file is never dropped; and
//!   2. the `serialize` hub fail-safe is CARRIED from the grounder, never silently dropped by the
//!      new tier-filter arm (the concern an earlier review raised: an implementer could drop it and
//!      every gate would stay green).
//!
//! The inside-out test cannot pin (2) non-vacuously: it grounds through a grep grounder whose
//! `serialize` is ALWAYS `false`, so `recorded == grep` is `false == false` and would still pass if
//! the graph arm hard-coded `serialize: false`. These tests inject a STRUCTURAL grounder double
//! whose radius carries `serialize: true` (and a non-empty `index_stamp`, so the audit is emitted at
//! all), populate the unified graph through the public `contextgraph` event API (the same serialized
//! events a real run folds), drive the public `conductor::run` entry, and assert on the recorded
//! `BlastRadiusComputed` event. They exercise the new cross-module seam end-to-end:
//! `grounded_blast_radius` reading the injected `graph` port, tier-filtering the ONE subgraph, and
//! the two-view radius reaching the serialized audit.
//!
//! The tier filter and the audit fields are always compiled, so these guard the boundary in BOTH
//! feature lanes.

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM, TYPE_BLAST_RADIUS_COMPUTED,
};
use rigger::config::{AgentDef, Config, Stage};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Projection, TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::gate::ExecRunner;
use rigger::grounder::{BlastRadius, Grounder, Ref};
use serde_json::{json, Value};

/// A driver that returns an empty result without doing anything. The blast radius is RECORDED before
/// the spawn (`run_stage`), so the run's terminal disposition is irrelevant to what this periphery
/// layer observes - it only needs the run to reach the record.
#[derive(Default)]
struct NoopDriver;

impl AgentDriver for NoopDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        _opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        Ok(AgentResult {
            output: String::new(),
            resolved_model: String::new(),
        })
    }
}

/// A STRUCTURAL grounder double - the shape criterion 2's graph arm and the audit actually key off,
/// which a grep grounder cannot stand in for:
///
///   - it seeds the traversal on `combat.rs` (so the graph `subgraph(seed, 2)` reads the tiered
///     neighborhood populated below);
///   - its `blast_radius` is the GREP FLOOR the tier-filter arm must union into `safe`: it carries a
///     `grep_only.rs` file the graph does not, and it flags `serialize: true` (the hub fail-safe the
///     arm must carry). A real grep grounder's `serialize` is always `false`, which is exactly why
///     the inside-out test cannot pin the carry non-vacuously;
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
            // The hub fail-safe. A grep grounder can never set this, so an injected structural
            // double is the only way to prove the graph arm carries a TRUE verdict through.
            serialize: true,
        }
    }

    fn index_stamp(&self) -> String {
        // Non-empty: the structural-active signal that makes the conductor emit the audit at all.
        "test-index-stamp/v1".into()
    }
}

/// Fold one event, built from its serialized JSON payload, into the graph at `pos` - the public
/// event API a real run folds through.
fn fold(g: &Projector, pos: &mut u64, type_: &str, payload: Value) {
    *pos += 1;
    let mut e = Event::new(type_, serde_json::to_vec(&payload).unwrap());
    e.position = *pos;
    g.apply(&e).unwrap();
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
    fold(
        &g,
        &mut pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        json!({ "file": "combat.rs", "name": "apply_damage", "kind": "function", "line": 1, "lang": "rust" }),
    );
    // ... and util.rs defines shared, so combat.rs's reference to it resolves cross-file (INFERRED).
    fold(
        &g,
        &mut pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        json!({ "file": "util.rs", "name": "shared", "kind": "function", "line": 1, "lang": "rust" }),
    );
    // combat.rs references apply_damage (same-file: EXTRACTED), shared (cross-file: INFERRED), and
    // magic (defined nowhere: AMBIGUOUS) - one reference per confidence tier.
    for name in ["apply_damage", "shared", "magic"] {
        fold(
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
/// recorded `BlastRadiusComputed.safe` stays a SUPERSET of the grounder's grep union. The grep floor
/// carries `grep_only.rs`, a file the graph traversal never reaches; the tier-filter arm UNIONS the
/// grep radius in, so `grep_only.rs` must survive into the recorded `safe`.
///
/// Non-vacuous: if the graph arm returned only the tier-filter's `safe` (dropping the grep union),
/// `grep_only.rs` would vanish from the recorded event and this assertion would fail - so it guards
/// exactly the "never narrow below the grep union" invariant against a real record path.
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
        "the tier-filter arm must UNION the grep radius, so the grep-only file survives into the \
         recorded safe (section 2.4: safe never narrows below the grep union); got {safe:?}"
    );
}

/// The `serialize` hub fail-safe, pinned at the PUBLIC serialized boundary: the grounder's radius
/// flags `serialize: true`, and the recorded `BlastRadiusComputed.serialize` on the GRAPH path (the
/// branch that constructs the new two-view radius) must carry that verdict through - never silently
/// drop it.
///
/// This is the assertion the inside-out unit test cannot make non-vacuously: it grounds through a
/// grep grounder whose `serialize` is always `false`, so its `recorded == grep` check is
/// `false == false` and would pass even if the graph arm hard-coded `serialize: false`. Here the
/// injected structural double carries `true`, so hard-coding `serialize: false` in the graph arm
/// would flip the recorded value and redden this test.
#[test]
fn the_graph_path_carries_the_hub_serialize_verdict_into_the_recorded_audit() {
    let graph = tiered_projector();
    let payload = recorded_blast_radius(Some(&graph))
        .expect("a BlastRadiusComputed audit IS emitted under the structural grounder");

    assert_eq!(
        payload["serialize"].as_bool(),
        Some(true),
        "the graph/tier-filter arm must carry the grounder's serialize hub verdict into the \
         recorded audit, never drop it; payload was {payload}"
    );
}

/// The `graph: None` fallback, pinned at the same serialized boundary: with no graph the recorded
/// radius is the grounder's radius verbatim - the tier-filter arm is skipped and the grep floor
/// (including `grep_only.rs`) and the `serialize` verdict pass through unchanged. This is the
/// precondition that makes the graph-path tests above meaningful: the graph arm is what runs the new
/// code, and it must preserve everything the fallback preserves (it may only WIDEN `safe`, never
/// narrow it, and never drop `serialize`).
#[test]
fn the_no_graph_fallback_records_the_grounder_radius_verbatim() {
    let payload = recorded_blast_radius(None)
        .expect("a BlastRadiusComputed audit IS emitted under the structural grounder");
    let safe = safe_of(&payload);

    assert!(
        safe.contains(&"combat.rs".to_string()) && safe.contains(&GREP_ONLY_FILE.to_string()),
        "the no-graph fallback records the grounder's grep radius verbatim; got {safe:?}"
    );
    assert_eq!(
        payload["serialize"].as_bool(),
        Some(true),
        "the no-graph fallback carries the grounder's serialize verdict; payload was {payload}"
    );
}
