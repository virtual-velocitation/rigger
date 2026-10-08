//! Periphery for spec 103, criterion 2 (NO UNGATED FAN-OUT UNIT): the runtime invariant's
//! authored-ungated-workflow carve-out AND its unmatched-sub-unit blind spot
//! (`adv-u103c2-guard-blind-to-unmatched-subunit`), both driven through the REAL production
//! wiring `conductor::run` itself performs - never through the private
//! `assert_no_ungated_fanout_unit` function the implementer's own `mod tests` call directly
//! with hand-built `stages`/`fanout_criteria`/`fanout_template_gates` maps.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! The implementer's `mod tests` (src/conductor.rs) proves the GUARD's decision logic in
//! isolation: `ungated_fanout_unit_under_an_ungated_template_is_not_flagged` hand-builds a
//! `fanout_criteria`/`fanout_template_gates` pair and calls `assert_no_ungated_fanout_unit`
//! directly, proving the FUNCTION tolerates an empty template-gates entry. It proves nothing
//! about the two REAL call sites `conductor::run`'s own wave loop wires it through: the
//! template-gates capture at the exact moment the template stage is consumed
//! (`fanout_template_gates.insert(template_name, template.gates.clone())`) and the two
//! `assert_no_ungated_fanout_unit` calls guarding each `run_wave`. A bug in either wiring
//! point - the capture keyed on the wrong name, the map never threaded through to the second
//! call site, the check firing before the real per-criterion decomposition it depends on -
//! would leave the implementer's own isolated test green while a REAL authored-ungated
//! fan-out spec could no longer run to completion at all. The first test below drives the
//! real `conductor::run` entry, real `deps.criteria`, and real criterion-driven baseline-unit
//! synthesis (`baseline_units`, never a hand-set `Stage.criterion_id`) end to end, proving the
//! genuine article: an authored fan-out template with NO gates at all still decomposes,
//! spawns, and integrates every one of its baseline units, through the actual
//! capture-then-check code path, not a stand-in for it.
//!
//! The SAME structural gap applies to the blind-spot fix (`Stage::unmatched_fanout_proposal`):
//! the implementer's own new white-box test
//! (`a_genuinely_new_proposal_with_no_gates_refuses_to_spawn_ungated`, src/conductor.rs `mod
//! tests`) already drives `run()` end to end and proves the fix from INSIDE the module the
//! code under test lives in - but it cannot attest to the fix surviving the real crate
//! boundary, since it can see (and could accidentally depend on) `conductor`'s private
//! internals. The second test below proves the identical scenario - a worker proposing a
//! genuinely-new sub-unit with no `gates` of its own, under a GATED fan-out template - from
//! this external, black-box test crate, through only `rigger`'s public API.
//!
//! NOT owned: the guard's decision-table logic itself (which combination of gated/ungated
//! template and gated/ungated unit fails vs. passes) - that is the implementer's own `mod
//! tests` in src/conductor.rs, proven directly against the pure function. This file owns only
//! whether the REAL wiring reaches the same (correct) answer for the two scenarios a real
//! spec can actually produce on the production path: the authored-ungated carve-out, and the
//! unmatched-sub-unit blind spot the fix closes.

mod common;
use common::fixtures::NoopDriver;

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM, TYPE_UNIT_PROPOSED,
};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::ledger::{Status, TYPE_UNIT_STARTED};
use serde_json::{json, Value};

/// A real, criteria-driven fan-out decomposition against a template that declares NO gates at
/// all must still run every synthesized baseline unit to completion - the Design's own
/// "empty template gates - a workflow authored with no gates keeps running ungated" carve-out,
/// proven through `conductor::run`'s REAL template-gates capture and its two real
/// `assert_no_ungated_fanout_unit` call sites, not through a stand-in for either.
#[test]
fn a_real_ungated_fanout_template_decomposes_and_every_baseline_unit_still_integrates() {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    // No `cfg.workflow.gates` entries at all: the template below declares none, so there is
    // nothing for a spawned unit's (empty) gate list to name.
    cfg.workflow.stages.insert(
        "implement".into(),
        Stage {
            name: "implement".into(),
            agent: "worker".into(),
            strategy: "fan-out".into(),
            gates: Vec::new(),
            on_pass: "merge".into(),
            coverage: "each unit is implemented and integrates, ungated".into(),
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    let driver = NoopDriver;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &rigger::gate::ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        // Two real criteria, so the real `baseline_units` synthesis produces two independent
        // fan-out units sharing the one ungated template - proving the carve-out per-unit, not
        // merely for a single coincidental spawn.
        criteria: vec![
            "the auth module lands".to_string(),
            "the billing module lands".to_string(),
        ],
        log: &|_| {},
        hash_blob: &|_| Ok(String::new()),
    };
    let rs = run(&cfg, &deps).expect(
        "an authored ungated fan-out template must run to completion through the real \
         capture-then-check wiring, never be refused by the backstop invariant meant only for \
         a template that DOES declare gates",
    );

    assert!(
        !rs.units.contains_key("implement"),
        "the bare template is a template, not a unit; it must not run as `implement` itself"
    );
    assert_eq!(
        rs.units.len(),
        2,
        "both criteria must synthesize their own baseline unit; got {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    for (name, unit) in &rs.units {
        assert_eq!(
            unit.status,
            Status::Integrated,
            "unit '{name}' must integrate - a real ungated fan-out template must never be \
             blocked from running by an invariant meant to catch a DROPPED gate list, not an \
             authored one"
        );
    }

    // The literal claim at the event-log boundary: the run recorded exactly one real
    // `UnitStarted` per criterion (read straight off the log, never asserted by construction),
    // proving the invariant's two call sites never short-circuited the wave before either
    // baseline unit could even start.
    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let started_ids: Vec<String> = events
        .iter()
        .filter(|e| e.type_ == TYPE_UNIT_STARTED)
        .map(|e| serde_json::from_slice::<Value>(&e.data).unwrap())
        .map(|v| {
            v.get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert_eq!(
        started_ids.len(),
        2,
        "one UnitStarted per baseline unit; got {started_ids:?}"
    );
}

/// A driver whose every spawn ALSO emits one real `TYPE_UNIT_PROPOSED` event for a
/// brand-new sub-unit ("new-subunit") whose criterion matches none of the run's baseline
/// criteria, and whose JSON payload OMITS `gates` entirely - the real shape a proposing
/// worker's own JSON can take, never a hand-built `Stage`. Mirrors the driver-per-spawn
/// convention the implementer's own `Stub` test double already establishes for this exact
/// scenario (`a_genuinely_new_proposal_with_no_gates_still_spawns_gated_via_template_
/// inheritance`, src/conductor.rs `mod tests`).
#[derive(Default)]
struct UnmatchedProposalWorker;

impl AgentDriver for UnmatchedProposalWorker {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        _opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        emit(
            TYPE_UNIT_PROPOSED,
            json!({
                "id": "new-subunit",
                "agent": "worker",
                "criterion": "an entirely separate concern the spec never lists",
                // `gates` omitted entirely - the defect shape adv-u103c2-guard-blind-to-
                // unmatched-subunit found.
            }),
        )?;
        Ok(AgentResult {
            output: String::new(),
            resolved_model: String::new(),
            ..Default::default()
        })
    }
}

/// THE BLIND SPOT ITSELF (spec 103, criterion 2,
/// `adv-u103c2-guard-blind-to-unmatched-subunit`): a genuinely-new sub-unit a worker
/// proposes mid-run, under a GATED fan-out template, with no `gates` of its own, must
/// never spawn ungated - proven here through the REAL `conductor::run` entry, the REAL
/// `harvest_proposed` ADD path, and the REAL `assert_no_ungated_fanout_unit` wiring,
/// driven from this external (black-box) test crate through only the crate's public API -
/// completing, at the periphery, the coverage the adversary's finding demanded once the
/// implementer closed the code gap (`Stage::unmatched_fanout_proposal`).
///
/// UPDATED at merge time (decision u103c2-merge-gate-inheritance-obsoletes-guard-
/// reproduction, mirrors the identical rename in `src/conductor.rs`'s own `mod tests`):
/// criterion 1's already-landed gate-inheritance fix (`harvest_proposed`'s
/// `union_gates`) now unconditionally seeds this stage with the template's own gates
/// before `assert_no_ungated_fanout_unit` ever runs, so the omitted-gates shape below can
/// no longer reach the guard with an empty gate list through this real `run()` entry
/// point - the guard itself stays covered directly (src/conductor.rs `mod tests`,
/// hand-built `stages`/`fanout_criteria`/`fanout_template_gates`). What this test proves
/// now is the combined, still-safe outcome: never ungated, gated by inheritance instead.
#[test]
fn a_genuinely_new_unmatched_proposal_under_a_gated_template_spawns_gated_via_inheritance() {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    // A gated template this time - the OPPOSITE of the carve-out test above - so the
    // guard has a non-empty template gate list to hold an ungated unit to.
    cfg.workflow.gates.insert(
        "ok".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
            requires: Vec::new(),
        },
    );
    cfg.workflow.stages.insert(
        "implement".into(),
        Stage {
            name: "implement".into(),
            agent: "worker".into(),
            strategy: "fan-out".into(),
            gates: vec!["ok".into()],
            on_pass: "merge".into(),
            coverage: "each unit is implemented and gated".into(),
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    let driver = UnmatchedProposalWorker;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &rigger::gate::ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: vec!["the auth module lands".to_string()],
        log: &|_| {},
        hash_blob: &|_| Ok(String::new()),
    };

    let rs = run(&cfg, &deps).unwrap_or_else(|e| {
        panic!(
            "a genuinely-new unmatched sub-unit under a GATED fan-out template must \
             inherit that template's gates and integrate, not be refused; got error: {:?}",
            e.0
        )
    });
    assert_eq!(
        rs.units["new-subunit"].status,
        Status::Integrated,
        "a genuinely-new unmatched sub-unit with no gates of its own must still run to \
         completion, gated by inheritance rather than refused"
    );
    assert_eq!(
        rs.units["new-subunit"]
            .evidence
            .get("verified")
            .map(String::as_str),
        Some("gates passed: ok"),
        "the unit must have actually run the gated template's inherited 'ok' gate, not \
         spawned ungated; got {:?}",
        rs.units["new-subunit"].evidence
    );
}
