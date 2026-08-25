//! Periphery (integration) test for spec 61 criterion 7 (MODEL PINNING), unit u61c7b:
//! `--model <tier>=<id>` pins the named review tier's agent(s) to a model id for one run,
//! leaving every other tier and the loaded config untouched, and the canary scorecard
//! records a run-level header - binary build, corpus content hash, and every tier's
//! ACTUALLY-resolved model id (from `AgentResult::resolved_model`, never a configured
//! alias) - so a pinned A/B arm is auditable from the scorecard alone.
//!
//! The implementer's own unit tests pin `apply_model_pins` in isolation against a hand-
//! built `Config`/`ReviewPanel` (never spawning anything, so a pin that never actually
//! reaches a spawned agent would still pass), and pin `CanaryHeader`'s wire round trip and
//! `metrics::project_canary`'s fold of it against a HAND-TYPED JSON event string built with
//! `format!` to match the wire shape from memory - the identical gap pattern the FINDINGS
//! VOLUME criterion's own periphery closed for its field
//! (`tests/canary_findings_volume_periphery.rs`): a wire-shape disagreement between
//! `CanaryHeader::to_event`'s REAL output and a hand-typed fixture would pass every one of
//! those tests while silently losing the header in a live run. This suite drives
//! `run_canary` - the public entry the shipped `rigger canary` command calls - with a
//! driver that reports back the model id its OWN incoming `AgentDef` actually carries
//! (unlike canary.rs's private `#[cfg(test)]` `Scripted` driver, which returns one fixed
//! `resolved_model` string regardless of which agent it was asked to run), over a config
//! where every tier starts on its OWN distinct default model and only the lens tier is
//! pinned - proving the pin changes what the SPAWNED agent resolves to for that tier alone,
//! that the header event genuinely produced from that run round-trips through the real
//! store, and that `metrics::project_canary` folds those genuinely-produced wire events
//! (never a hand-typed fixture) into the same values.

use rigger::canary::{
    apply_model_pins, corpus_hash, record_header, run_canary, CanaryHeader, CanaryItem,
    CanaryOutcome, ModelPins, STREAM, TIER_ADVERSARY, TIER_LENS,
};
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, ReviewPanel};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::spawn::ROLE_ADJUDICATOR;
use serde_json::Value;

/// A driver that reports back the exact model id its OWN incoming `AgentDef` carries.
/// Written from scratch for this outside-in layer (it does not, and cannot, reuse
/// canary.rs's own `#[cfg(test)]`-private `Scripted` driver, which ignores the incoming
/// agent's model entirely). Every spawn approves - the MODEL PINNING criterion is about
/// which model id resolves, not review outcomes, which the FINDINGS VOLUME and NO FAKE
/// ZEROS criteria's own periphery already own.
struct EchoModelDriver;

impl AgentDriver for EchoModelDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        _prompt: &str,
        _opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        Ok(AgentResult {
            output: "{\"verdict\":\"approve\"}".to_string(),
            resolved_model: a.model.clone(),
        })
    }
}

fn agent(id: &str, model: &str, ladder: &[&str]) -> AgentDef {
    AgentDef {
        id: id.to_string(),
        model: model.to_string(),
        model_ladder: ladder.iter().map(|s| (*s).to_string()).collect(),
        ..Default::default()
    }
}

fn item(id: &str) -> CanaryItem {
    CanaryItem {
        id: id.into(),
        defect_class: "none".into(),
        planted: false,
        anchor: String::new(),
        expected_verdict: "approve".into(),
        expected_tier: String::new(),
        review: format!("fn {id}() {{}}"),
    }
}

/// Drives the pin from `apply_model_pins` through `run_canary`'s real spawn path, records
/// the resulting header through the real store, and folds it back through
/// `metrics::project_canary` - proving the criterion's full surface end to end rather than
/// each seam in isolation against its own fixture.
#[test]
fn model_pinning_takes_effect_through_run_canary_and_the_header_round_trips_and_folds() {
    let mut cfg = Config::default();
    // The lens agent starts with its OWN default model AND a non-empty ladder, so pinning
    // it also proves `apply_model_pins` clears the ladder (a lingering ladder entry could
    // otherwise mask a broken pin by coincidentally matching rung 0).
    cfg.agents.insert(
        "lens-a".into(),
        agent("lens-a", "lens-default", &["haiku", "opus"]),
    );
    cfg.agents
        .insert("adv".into(), agent("adv", "adv-default", &[]));
    cfg.agents
        .insert("adj".into(), agent("adj", "adj-default", &[]));
    let panel = ReviewPanel {
        lenses: vec!["lens-a".into()],
        adversary: "adv".into(),
        adjudicator: "adj".into(),
        tiers: None,
    };

    let mut pins = ModelPins::new();
    pins.insert(TIER_LENS.to_string(), "pinned-lens-id".to_string());
    let pinned_cfg = apply_model_pins(&cfg, &panel, &pins);

    let corpus = vec![item("only")];
    let store = Store::open(":memory:").expect("an in-memory store opens");
    let report = run_canary(&store, &EchoModelDriver, &pinned_cfg, &panel, &corpus, 1)
        .expect("run_canary succeeds through the public entry");

    assert_eq!(
        report.resolved_models.get(TIER_LENS),
        Some(&"pinned-lens-id".to_string()),
        "the pinned tier's SPAWNED agent actually resolves to the pinned id, not the \
         config's default - proven at the spawn boundary, not by inspecting Config alone"
    );
    assert_eq!(
        report.resolved_models.get(TIER_ADVERSARY),
        Some(&"adv-default".to_string()),
        "an un-pinned tier's spawned agent still resolves to its OWN configured default"
    );
    assert_eq!(
        report.resolved_models.get(ROLE_ADJUDICATOR),
        Some(&"adj-default".to_string()),
        "the adjudicator, also un-pinned, resolves to its own configured default too"
    );

    // Build and record the header exactly as `cmd_canary` does: from the run's REAL
    // resolved_models, through the public `corpus_hash` and `record_header` entries.
    let hash = corpus_hash(&corpus);
    let header = CanaryHeader {
        binary_build: "rigger 9.9.9 (build periphery-test)".to_string(),
        corpus_hash: hash,
        resolved_models: report.resolved_models.clone(),
    };
    record_header(&store, &report.batch, &header).expect("the header event records");

    let events = store
        .read_stream(STREAM, 0, Direction::Forward)
        .expect("the canary stream reads back");
    assert_eq!(events.len(), 3, "batch marker + one outcome + the header");

    let decoded_header = CanaryHeader::from_event(events.last().unwrap())
        .expect("the trailing event decodes as the header");
    assert_eq!(
        decoded_header, header,
        "the header - built from the run's REAL resolved_models, not a hand-typed fixture \
         - round-trips byte-for-byte through the real store"
    );
    // Cross-decoder isolation: the header and a per-item outcome share the SAME
    // TYPE_UNIT_STATUS event type, distinguished only by their "status" token. Neither
    // decoder may accept the other shape - a collision here would silently corrupt
    // `project_canary`'s item count or drop the header.
    assert!(
        CanaryOutcome::from_event(events.last().unwrap()).is_none(),
        "the header event must NOT also decode as a per-item outcome"
    );
    let outcome_event = &events[1];
    assert!(
        CanaryHeader::from_event(outcome_event).is_none(),
        "the reverse direction: a real per-item outcome event must not decode as a header"
    );

    // The cross-module fold arm: metrics::project_canary, driven over the REAL recorded
    // events (never a hand-typed JSON fixture), must recover the exact header fields -
    // proving canary.rs's actual wire output and metrics.rs's fold agree on the shape, a
    // disagreement neither module's own isolated unit tests could see.
    let m = rigger::metrics::project_canary(&events);
    assert_eq!(m.binary_build, header.binary_build);
    assert_eq!(m.corpus_hash, header.corpus_hash);
    assert_eq!(
        m.resolved_models.get(TIER_LENS),
        Some(&"pinned-lens-id".to_string())
    );
    assert_eq!(
        m.resolved_models.get(TIER_ADVERSARY),
        Some(&"adv-default".to_string())
    );
    assert_eq!(
        m.resolved_models.get(ROLE_ADJUDICATOR),
        Some(&"adj-default".to_string())
    );

    // The ORIGINAL config passed to apply_model_pins is never mutated by anything this
    // chain did with it - the pin exists for this run only.
    assert_eq!(cfg.agents["lens-a"].model, "lens-default");
    assert_eq!(
        cfg.agents["lens-a"].model_ladder,
        vec!["haiku".to_string(), "opus".to_string()]
    );
}
