//! Periphery (contract / API / integration) tests for spec 60 criterion 2: RUN-SCOPING SURVIVES
//! the project-scoped ingest dedup. A non-ingest replay key recorded by a PRIOR run must never
//! suppress the current run's own keyed emit, and a non-derived event is ineligible for the
//! project-scoped arm of the seed whatever its key looks like.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface, so they guard the boundary the
//! inside-out unit tests are structurally blind to. Criterion 2 adds no production code of its
//! own (the predicate it pins is criterion 1's), so this layer covers the CONTRACT at the crate
//! edge rather than a new API. What the existing layers cannot see:
//!
//! 1. NOTHING outside the crate drives the run entry TWICE OVER ONE STORE. The unit layer's proof
//!    (`conductor::tests::a_prior_runs_non_ingest_replay_key_never_suppresses_this_runs_keyed_emit`)
//!    lives inside `mod tests` and reaches the crate-private key authority `gate_verdict_key` to
//!    compute the key it then looks for. An external consumer CANNOT compute that key - the
//!    authority is private - so its only view of the boundary is what the log RECORDS. These tests
//!    take the keys from the first run's own recorded slice and require the second run to record
//!    them again, so the property is stated in the only terms a consumer of the crate has.
//! 2. The sibling criterion-1 periphery file (`dedup_seeding_periphery.rs`) pins the type gate over
//!    HAND-SPELLED content keys and states in its own header that "the run-scoping boundary for
//!    NON-ingest keys (criterion 2) ... [is] owned by sibling units and [is] deliberately not
//!    exercised here". A hand-spelled key cannot prove that a REAL RUN ever mints one of that
//!    shape; if it never did, the type gate would be guarding nothing on the run path. These tests
//!    close that with the run's OWN minted lifecycle key, read back off the log.
//! 3. The seam is `conductor::run` -> `ingest::project_scoped_replay_keys` across a module
//!    boundary, observed through the public `eventstore` read path. Neither module's own tests see
//!    both halves at once from outside.
//!
//! Nothing here is `symbols`-gated: the seeding partition and the predicate are compiled and
//! called in BOTH feature lanes, so the boundary is locked in both.
//!
//! Scope is strictly criterion 2. The seeding fix itself (criterion 1), the change/revert path
//! (criterion 3) and the store-layer append guard (criterion 4) are owned by sibling units and are
//! deliberately not exercised here.

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, META_REPLAY_KEY, STREAM,
};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::contextgraph::{TYPE_EDGE_INFERRED, TYPE_GATE_VERDICT};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::gate::ExecRunner;
use rigger::ingest::project_scoped_replay_keys;
use rigger::ledger::{Status, TYPE_UNIT_STARTED};
use rigger::run::{current_run, TYPE_RUN_STARTED};
use serde_json::Value;
use std::collections::BTreeSet;

/// The stage id is `gc` - the CODE INGEST'S OWN batch prefix - and the gate id carries an `@`, the
/// content key format's own generation separator. Between them the gate-verdict key this run really
/// mints parses as a `<prefix>/<file>@<hash>#<i>` content key IN FULL, so nothing but the TYPE test
/// keeps this run's own recorded lifecycle keys out of the project-scoped arm of the seed. That the
/// minted key really is that shape is ASSERTED below off the recorded log, never assumed.
const UNIT: &str = "gc";
const GATE: &str = "g@h1";

/// A driver that does nothing and reports nothing. Criterion 2 is about what the RUN records at its
/// own seeding seam, so the agent's work is irrelevant - the stage only has to reach its gates and
/// integrate, which an empty result and a `true` gate do.
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

/// One campaign over `store`, differing from its sibling ONLY in its criterion - so the run
/// authority MINTS a second `RunStarted` (a genuinely fresh run) while the stage keeps its id, and
/// every lifecycle key the second run computes collides EXACTLY with the first run's records. That
/// collision is the whole point: run-scoping is what makes the second run emit anyway.
fn campaign(store: &Store, criterion: &str) -> Status {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "a".into(),
        AgentDef {
            id: "a".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        GATE.into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.stages.insert(
        UNIT.into(),
        Stage {
            name: UNIT.into(),
            agent: "a".into(),
            coverage: criterion.into(),
            gates: vec![GATE.into()],
            ..Default::default()
        },
    );
    let driver = NoopDriver;
    let deps = Deps {
        store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    run(&cfg, &deps).unwrap().units[UNIT].status
}

/// Every replay key the events of `type_` in `events` carry, addressed through the crate's public
/// `META_REPLAY_KEY` - the only view of a run's keys an external consumer has, since the key
/// authority itself is crate-private.
fn keys_of(events: &[Event], type_: &str) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.type_ == type_)
        .filter_map(|e| e.meta.get(META_REPLAY_KEY).cloned())
        .collect()
}

fn read(store: &Store) -> Vec<Event> {
    store.read_stream(STREAM, 0, Direction::Forward).unwrap()
}

fn count_of_type(events: &[Event], type_: &str) -> usize {
    events.iter().filter(|e| e.type_ == type_).count()
}

/// CONTRACT at the crate boundary: a run's own keyed lifecycle emits are RUN-scoped, so what a
/// PRIOR run recorded under the very same key never suppresses them.
///
/// This is the Gap 11 boundary, and criterion 1 turned the seeding into a PARTITION over two
/// scopes rather than widening one: derived-index keys became project-scoped, and every OTHER
/// replay key - unit lifecycle, gate verdicts, breaker trips - stayed run-scoped. Widen the
/// run-scoped half to the whole stream and a new campaign over an existing store silently loses
/// its own lifecycle: the `UnitStarted` reads as a replay of the previous campaign's, so the run
/// records no start and no gate verdict, and the operator metrics count a unit that never appeared
/// to begin.
///
/// The proof is stated the way a CONSUMER must state it. `gate_verdict_key` is crate-private, so
/// nothing outside the crate can compute the key to look for; this takes the keys the FIRST run
/// actually recorded off the log and requires the SECOND run to record those same keys again,
/// INSIDE its own slice - so the log genuinely gained a second copy rather than merely still
/// holding the first.
#[test]
fn a_prior_runs_recorded_lifecycle_keys_do_not_suppress_the_next_runs_own_emits() {
    let store = Store::open(":memory:").unwrap();

    assert_eq!(
        campaign(&store, "first criterion"),
        Status::Integrated,
        "sanity: the first campaign must actually run the unit, else it records no key to collide \
         with"
    );
    let after_one = read(&store);
    let first_started = keys_of(current_run(&after_one), TYPE_UNIT_STARTED);
    let first_verdict = keys_of(current_run(&after_one), TYPE_GATE_VERDICT);
    assert_eq!(
        first_started.len(),
        1,
        "sanity: the first run must record exactly one keyed {TYPE_UNIT_STARTED}; got \
         {first_started:?}"
    );
    assert_eq!(
        first_verdict.len(),
        1,
        "sanity: the first run must record exactly one keyed {TYPE_GATE_VERDICT}; got \
         {first_verdict:?}"
    );

    assert_eq!(
        campaign(&store, "second criterion"),
        Status::Integrated,
        "the second campaign must run the unit again - a prior run's residue is not this run's work"
    );
    let after_two = read(&store);
    assert_eq!(
        count_of_type(&after_two, TYPE_RUN_STARTED),
        2,
        "the second campaign must mint its OWN fresh {TYPE_RUN_STARTED}, else the two campaigns \
         are one run and this test proves nothing"
    );

    let second = current_run(&after_two);
    for (type_, first_keys) in [
        (TYPE_UNIT_STARTED, &first_started),
        (TYPE_GATE_VERDICT, &first_verdict),
    ] {
        let key = &first_keys[0];
        assert!(
            keys_of(second, type_).contains(key),
            "a {type_} key recorded by a PRIOR run must not suppress this run's own keyed emit: \
             the second run emitted no {type_} under {key}; it recorded {:?}",
            keys_of(second, type_)
        );
        assert_eq!(
            keys_of(&after_two, type_)
                .iter()
                .filter(|k| *k == key)
                .count(),
            2,
            "the log must carry {key} once PER RUN, not one shared copy across runs"
        );
    }
}

/// CONTRACT at the crate boundary: what keeps a lifecycle fact out of the project-scoped ARM OF
/// THE SEED is its TYPE, not the shape of its key - proved against the keys a REAL RUN MINTS
/// rather than hand-spelled ones.
///
/// The claim stops at that arm, deliberately, because that is where the type test lives
/// (`ingest::project_scoped_replay_keys` skips a non-derived event before it ever reads that
/// event's key). What both arms pour into is ONE flat key set, and the suppression DECISION
/// downstream is a plain membership test on it with no type test of its own - so this pins what
/// ENTERS the set, and never the stronger claim that a lifecycle emit cannot be suppressed.
///
/// The sibling criterion-1 periphery layer pins the type gate over the full public event vocabulary
/// with keys the TEST spells. That leaves one thing unproven and it is the thing that makes the
/// gate load-bearing on the run path: that a real run ever mints a lifecycle key of the eligible
/// shape at all. If it never did, the type test would be guarding a case that cannot arise, and a
/// predicate that read key SPELLINGS instead of event types would pass every existing test.
///
/// So the fixture is built to mint one. The stage id is `gc`, the code ingest's own batch prefix,
/// and the gate id carries the format's `@` separator, so the gate-verdict key the run really
/// records parses as `<prefix>/<file>@<hash>#<i>` in full. Two assertions in opposite directions:
/// that key, re-stamped onto a DERIVED-index event, IS returned by the public predicate (so the
/// fixture is not vacuous and the predicate really was offered this key); and over the REAL log, in
/// which that key belongs to a `GateVerdict`, the predicate returns NOTHING. Type first - the key
/// never reaches the shape comparison inside the predicate.
#[test]
fn the_keys_a_real_run_mints_are_eligible_in_shape_yet_type_keeps_them_out_of_the_seed_arm() {
    let store = Store::open(":memory:").unwrap();
    assert_eq!(campaign(&store, "only criterion"), Status::Integrated);
    let log = read(&store);

    let verdict_keys: BTreeSet<String> = keys_of(&log, TYPE_GATE_VERDICT).into_iter().collect();
    assert!(
        !verdict_keys.is_empty(),
        "sanity: the run must record a keyed {TYPE_GATE_VERDICT} for this proof to have a subject"
    );

    // DIRECTION ONE - the keys this run MINTED are genuinely content-key shaped. Re-stamped onto a
    // derived-index event, the public predicate returns every one of them. Without this the test
    // below would pass against a predicate that merely failed to parse the key.
    let as_derived: Vec<Event> = verdict_keys
        .iter()
        .map(|k| Event::new(TYPE_EDGE_INFERRED, Vec::new()).with_meta(META_REPLAY_KEY, k))
        .collect();
    let eligible = project_scoped_replay_keys(&as_derived);
    let missed: Vec<&String> = verdict_keys
        .iter()
        .filter(|k| !eligible.contains(*k))
        .collect();
    assert!(
        missed.is_empty(),
        "the fixture is vacuous unless the keys the run really mints parse as \
         <prefix>/<file>@<hash>#<i> content keys - fix the fixture (the stage id is the ingest \
         prefix and the gate id carries the `@`), not this assertion; unparsed: {missed:?}"
    );

    // DIRECTION TWO - and yet, over the REAL log where those same keys belong to `GateVerdict`
    // events, the predicate offers nothing at all. No domain or lifecycle event a run records
    // contributes a key to the project-scoped arm of the seed, however its key is spelled.
    assert!(
        project_scoped_replay_keys(&log).is_empty(),
        "a non-derived event is ineligible for the project-scoped arm of the seed whatever its \
         key looks like; over a real run's log the predicate returned {:?}",
        project_scoped_replay_keys(&log)
    );
}
