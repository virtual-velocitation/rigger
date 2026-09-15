//! Periphery for spec 80, unit c2 (criterion 2 - DELIVERY TO CONSUMERS): whether a criterion's
//! FULL extracted text - not just what `extract_criteria` itself returns (criterion 1's own
//! seam, pinned by `tests/criteria_extraction_periphery.rs`) - actually REACHES `self.deps.criteria`
//! and is what `UnitStarted.spec_criterion` carries for that criterion's baseline unit.
//!
//! What this file OWNS: driving the REAL extraction path (`rigger::spec::extract_criteria` over
//! the REAL committed `specs/62-dash-marker-lifecycle.md`, criterion 1's own three-physical-line
//! text with the OWNS sentence on the third line - the exact real-world input spec 80's Goal names
//! as the fresh run that proved the truncation live) through the REAL conductor delivery
//! mechanism: `conductor::Deps.criteria` -> the deterministic baseline-unit synthesis
//! (`conductor::run`'s fan-out-template expansion, §3.2) -> the resulting `UnitStarted` event's
//! `spec_criterion` field. This is "that criterion's baseline unit" in the criterion's own words:
//! the per-criterion unit the conductor itself synthesizes from `deps.criteria`, not a stage whose
//! `coverage` a test sets by hand (which would bypass the very delivery seam this criterion owns -
//! `baseline_units`, conductor.rs:9881-9905 - and prove nothing about it).
//!
//! NOT owned (criterion 1's own text: "the extractor itself is criterion 1's, NOT this one's";
//! spec 80's Design, BLAST RADIUS: "src/spec.rs only, plus tests"): whether `extract_criteria`
//! itself correctly joins the checkbox's continuation lines - that is
//! `tests/criteria_extraction_periphery.rs`'s pinned contract, reused here only as the REAL input
//! this file drives through the conductor, never re-derived or re-asserted independently.
//!
//! Every consumer this criterion's Notes name (unit titles, the grounding query,
//! `build_dag_critique_prompt`) reads the SAME `Stage::coverage` a baseline unit is given here, so
//! proving delivery into `coverage`/`spec_criterion` proves delivery to all of them at once; this
//! file does not re-walk each call site separately.

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::ledger::{Status, TYPE_UNIT_STARTED};
use serde_json::Value;
use std::path::Path;

/// A driver that does nothing and reports nothing: this criterion is about what the baseline-unit
/// synthesis RECORDS at the `UnitStarted` seam, not about agent behaviour - an empty result and a
/// `true` gate are enough to reach integration.
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

/// The exact real-world input spec 80's Goal names: specs/62's own criterion 1, extracted through
/// the SAME public `extract_criteria` call every real consumer (`main.rs::load_criteria`,
/// `conductor.rs`) uses - reading the file from disk and passing its text straight through, no
/// synthetic reshaping. Criterion 1's own periphery file pins that this call returns the FULL
/// three-line text; this helper only replays that call so the value this file drives through the
/// conductor is the genuine current output, not a copy that could silently drift from it.
fn real_spec_62_criterion_one() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("specs/62-dash-marker-lifecycle.md");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let criteria = rigger::spec::extract_criteria(&text);
    assert!(
        !criteria.is_empty(),
        "specs/62 must parse at least one Done-when criterion"
    );
    criteria[0].clone()
}

fn field(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

/// CONTRACT at the crate boundary: a criterion's FULL extracted text, driven through
/// `conductor::Deps.criteria` and the fan-out baseline-unit synthesis exactly as a real run does,
/// reaches that criterion's own `UnitStarted.spec_criterion` unmodified - proven against the real
/// specs/62 c1 text, whose third physical line carries an OWNS sentence a first-line-only
/// truncation would have dropped.
#[test]
fn baseline_unit_started_carries_the_full_multiline_criterion_including_its_owns_sentence() {
    let criterion = real_spec_62_criterion_one();

    // Sanity: this is genuinely the multi-line case the bug truncated, not a coincidentally
    // short criterion this run/build could regress to trivially. If this ever fires, specs/62's
    // own criterion 1 changed shape and this file's premise (and criterion 1's own periphery
    // pin) need revisiting together - it is not this test's job to re-derive that.
    assert!(
        criterion.contains("This criterion OWNS the write ordering."),
        "sanity: specs/62 criterion 1 must still carry its third-physical-line OWNS sentence for \
         this to be a meaningful multi-line-delivery proof; got: {criterion:?}"
    );
    let truncated_first_line =
        "a test proves MARKER FOLLOWS BIND: a dash start whose bind fails writes no marker and";
    assert_ne!(
        criterion, truncated_first_line,
        "sanity: the extraction this test drives through the conductor must not itself be \
         truncated to the first physical line - that would make this a no-op proof"
    );

    let mut cfg = Config::default();
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "ok".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    // The fan-out TEMPLATE: `deps.criteria` below drives the conductor's own baseline-unit
    // synthesis (`fan_out_template_name` + `baseline_units`, conductor.rs:9691-9905), which
    // replaces this template with ONE unit per criterion, each carrying the real criterion text
    // as its `coverage` - never this template's own label.
    cfg.workflow.stages.insert(
        "implement".into(),
        Stage {
            name: "implement".into(),
            agent: "worker".into(),
            strategy: "fan-out".into(),
            gates: vec!["ok".into()],
            on_pass: "merge".into(),
            coverage: "each unit is implemented and integrates".into(),
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
        // The real extraction path's output, fed straight into `deps.criteria` exactly as
        // `main.rs::load_criteria` -> the conductor's `Deps` construction does in production -
        // no synthetic reshaping between extraction and delivery.
        criteria: vec![criterion.clone()],
    };
    let rs = run(&cfg, &deps).unwrap();

    // The bare template was replaced by the synthesized baseline unit, never run as `implement`
    // itself.
    assert!(
        !rs.units.contains_key("implement"),
        "the fan-out template is a template, not a unit; it must not run as `implement`"
    );

    // The projected RunState's own unit carries the full criterion as its spec_criterion, and it
    // reached Integrated (so the baseline unit really ran, not merely got proposed).
    let unit =
        rs.units
            .values()
            .find(|u| u.spec_criterion == criterion)
            .unwrap_or_else(|| {
                panic!(
                "no baseline unit's spec_criterion matched the full delivered criterion; got: {:?}",
                rs.units.values().map(|u| &u.spec_criterion).collect::<Vec<_>>()
            )
            });
    assert_eq!(
        unit.status,
        Status::Integrated,
        "the criterion's baseline unit must run and integrate for this to prove real delivery, \
         not just a projected label"
    );

    // The literal claim: the RAW `UnitStarted` event this baseline unit recorded carries the
    // full text (including the third-line OWNS sentence) as its `spec_criterion` field - read
    // straight off the log, the same view any real consumer (conductor.rs:3333-3334) has.
    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let started: Vec<Value> = events
        .iter()
        .filter(|e| e.type_ == TYPE_UNIT_STARTED)
        .map(|e| serde_json::from_slice::<Value>(&e.data).unwrap())
        .collect();
    assert_eq!(
        started.len(),
        1,
        "exactly one baseline unit for the single criterion this run was given; got {started:?}"
    );
    let delivered = field(&started[0], "spec_criterion");
    assert_eq!(
        delivered, criterion,
        "UnitStarted.spec_criterion must carry the criterion's FULL extracted text unmodified - \
         got a value that diverges from what extract_criteria produced, which is exactly the \
         truncation-survives-delivery regression this criterion guards against"
    );
    assert!(
        delivered.contains("This criterion OWNS the write ordering."),
        "the third-physical-line OWNS sentence must survive all the way to UnitStarted.spec_criterion, \
         not just extraction; got: {delivered:?}"
    );
    assert_ne!(
        delivered, truncated_first_line,
        "UnitStarted.spec_criterion must not regress to first-physical-line-only truncation"
    );
}
