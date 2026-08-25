//! Periphery (integration) test for spec 61 criterion 3 (FALSE POSITIVES ARE FIRST-CLASS),
//! unit u61c3: `metrics::CanaryMetrics` gained `controls: u64` and
//! `control_false_positives: u64` - `project_canary`'s fold now counts every known-good
//! control item (`planted == false`) into `controls`, and the ones the adjudicator
//! INCORRECTLY rejected (`verdict_approved == false`) into `control_false_positives` -
//! computed from `CanaryOutcome`'s EXISTING `planted`/`verdict_approved` fields, no new
//! `CanaryOutcome` field.
//!
//! The implementer's own unit tests pin this at two PRIVATE/internal seams in isolation:
//! `project_canary` (metrics.rs, driven over a hand-rolled JSON event fixture typed by hand
//! to match the wire shape from memory via a local `canary_outcome` helper) and
//! `format_canary_stats` (main.rs, driven against a hand-built `CanaryMetrics` that was
//! never produced by `project_canary` itself). Neither drives the chain through the PUBLIC
//! production entry `run_canary` the shipped `rigger canary` command calls, so a wire-shape
//! disagreement between `CanaryOutcome::to_event`'s REAL output and either fixture would
//! pass both tests while silently miscounting `controls`/`control_false_positives` in a
//! live run.
//!
//! This suite drives `run_canary` with a scripted panel whose single lens raises a
//! reject-worthy finding for THREE items - a genuine planted defect and, mistakenly, two
//! known-good controls (the exact shape a false positive takes in a live run: the lens
//! flags something in a clean file, and the adjudicator, seeing the reject-worthy finding,
//! sends a correct unit back) - alongside one control the panel gets right. It then reads
//! the events actually recorded back out of the store, decodes them through the same
//! `CanaryOutcome::from_event` wire authority `rigger stats --canary` reads through, and
//! folds them via the public `metrics::project_canary` - proving `controls` and
//! `control_false_positives` survive the real wire round trip over four independently-
//! varying items, not just a fixture typed to match it.

use rigger::canary::{default_jobs, run_canary, CanaryItem, CanaryOutcome, STREAM};
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, ReviewPanel};
use rigger::contextgraph::TYPE_REVIEW_FINDING;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use serde_json::{json, Value};

/// The text this driver's scripted adjudicator looks for verbatim in its own prompt (which
/// embeds every finding's summary) to decide whether to reject - mirroring how a live
/// adjudicator's reject decision is driven by what it actually reads, never by the item's
/// own `planted` flag (which the driver never sees). This is the exact independence a false
/// positive requires: the panel can incorrectly reject an item that was never planted at
/// all.
const REJECT_MARKER: &str = "REJECT-THIS-ITEM";

/// A minimal scripted `AgentDriver` written from scratch for this outside-in layer (it does
/// not, and cannot, reuse canary.rs's own `#[cfg(test)]`-private driver). The lens's
/// behavior is routed by a marker embedded in the item's review body under test - the only
/// per-item signal a driver receives via the prompt - so two DIFFERENT known-good controls
/// can independently trigger the lens into mistakenly flagging them, while a third control
/// and the one planted item are routed separately.
struct FalsePositiveDriver;

impl AgentDriver for FalsePositiveDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        prompt: &str,
        _opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        match a.id.as_str() {
            "adj" => {
                let verdict = if prompt.contains(REJECT_MARKER) {
                    "reject"
                } else {
                    "approve"
                };
                return Ok(AgentResult {
                    output: format!("{{\"verdict\":\"{verdict}\"}}"),
                    resolved_model: String::new(),
                });
            }
            "lens" => {
                if prompt.contains("PLANTED-MARKER") {
                    // A genuine planted defect - the lens is right to flag it.
                    emit(
                        TYPE_REVIEW_FINDING,
                        json!({
                            "id": "f-planted",
                            "by": "lens",
                            "summary": format!("{REJECT_MARKER}: found the planted defect"),
                            "about": ["planted-secret.rs"],
                        }),
                    )?;
                } else if prompt.contains("BAD-CONTROL-A-MARKER") {
                    // A known-good control the lens MISTAKENLY flags - the first false
                    // positive.
                    emit(
                        TYPE_REVIEW_FINDING,
                        json!({
                            "id": "f-bad-a",
                            "by": "lens",
                            "summary": format!("{REJECT_MARKER}: this looks wrong"),
                            "about": ["clean-file-a.rs"],
                        }),
                    )?;
                } else if prompt.contains("BAD-CONTROL-B-MARKER") {
                    // A second, independent known-good control the lens ALSO mistakenly
                    // flags - a second false positive, so the summary counts more than one.
                    emit(
                        TYPE_REVIEW_FINDING,
                        json!({
                            "id": "f-bad-b",
                            "by": "lens",
                            "summary": format!("{REJECT_MARKER}: also looks wrong"),
                            "about": ["clean-file-b.rs"],
                        }),
                    )?;
                }
                // GOOD-CONTROL-MARKER: the lens raises nothing - this control is judged
                // correctly.
            }
            _ => {}
        }
        Ok(AgentResult {
            output: "reviewed".into(),
            resolved_model: String::new(),
        })
    }
}

fn agent(id: &str) -> AgentDef {
    AgentDef {
        id: id.to_string(),
        ..Default::default()
    }
}

fn cfg(ids: &[&str]) -> Config {
    let mut c = Config::default();
    for id in ids {
        c.agents.insert((*id).to_string(), agent(id));
    }
    c
}

fn panel() -> ReviewPanel {
    ReviewPanel {
        lenses: vec!["lens".to_string()],
        adversary: String::new(),
        adjudicator: "adj".to_string(),
        tiers: None,
    }
}

fn item(id: &str, anchor: &str, planted: bool, verdict: &str, marker: &str) -> CanaryItem {
    CanaryItem {
        id: id.into(),
        defect_class: if planted {
            "off-by-one".into()
        } else {
            "none".into()
        },
        planted,
        anchor: anchor.into(),
        expected_verdict: verdict.into(),
        expected_tier: String::new(),
        review: format!("fn {id}() {{ /* {marker} */ }}"),
    }
}

/// Drives `run_canary` over one planted defect and three independently-varying control
/// items and proves `metrics::project_canary`'s new fold arm counts EXACTLY the three
/// controls into `controls` (never the planted item, which has nothing to be a false
/// positive about) and EXACTLY the two wrongly-rejected ones into
/// `control_false_positives` - over real wire data recorded and read back through the
/// actual store, not a hand-typed fixture.
#[test]
fn run_canary_scores_false_positive_controls_and_project_canary_counts_them() {
    let cfg = cfg(&["lens", "adj"]);
    let panel = panel();

    let corpus = vec![
        // A genuine planted defect, correctly rejected - not a control at all, so it must
        // never contribute to `controls` or `control_false_positives`.
        item(
            "planted",
            "planted-secret.rs",
            true,
            "reject",
            "PLANTED-MARKER",
        ),
        // A known-good control the panel gets right: correctly approved, no false
        // positive.
        item("good-control", "", false, "approve", "GOOD-CONTROL-MARKER"),
        // A known-good control the panel WRONGLY rejects - THE false-positive case, first
        // instance (proving the count is not just "at least one").
        item(
            "bad-control-a",
            "",
            false,
            "approve",
            "BAD-CONTROL-A-MARKER",
        ),
        // A second, independently-triggered known-good control the panel ALSO wrongly
        // rejects - proving the fold SUMS false positives rather than saturating at one.
        item(
            "bad-control-b",
            "",
            false,
            "approve",
            "BAD-CONTROL-B-MARKER",
        ),
    ];

    let store = Store::open(":memory:").expect("an in-memory store opens");
    let report = run_canary(
        &store,
        &FalsePositiveDriver,
        &cfg,
        &panel,
        &corpus,
        default_jobs(),
    )
    .expect("run_canary succeeds through the public entry");

    assert_eq!(report.outcomes.len(), 4, "one outcome per corpus item");
    let by_id = |id: &str| -> &CanaryOutcome {
        report
            .outcomes
            .iter()
            .find(|o| o.id == id)
            .unwrap_or_else(|| panic!("no outcome recorded for item {id:?}"))
    };

    let planted = by_id("planted");
    assert!(planted.planted, "this item carries a planted defect");
    assert!(
        planted.verdict_correct && !planted.verdict_approved,
        "the lens correctly flagged the planted defect and the adjudicator rejected it"
    );

    let good = by_id("good-control");
    assert!(
        !good.planted && good.verdict_correct && good.verdict_approved,
        "a known-good control the panel correctly approved - not a false positive"
    );

    let bad_a = by_id("bad-control-a");
    assert!(
        !bad_a.planted && !bad_a.verdict_correct && !bad_a.verdict_approved,
        "a known-good control the panel wrongly rejected - a false positive"
    );

    let bad_b = by_id("bad-control-b");
    assert!(
        !bad_b.planted && !bad_b.verdict_correct && !bad_b.verdict_approved,
        "a second, independently-triggered known-good control the panel also wrongly \
         rejected - a second false positive"
    );

    // Read the events back through the real store and decode with the SAME wire-schema
    // authority `rigger stats --canary` reads through - proving the fold below runs over
    // genuine recorded wire data, not an in-process return value or a fixture typed by hand
    // to match it.
    let events = store
        .read_stream(STREAM, 0, Direction::Forward)
        .expect("the canary stream reads back");
    let decoded: Vec<CanaryOutcome> = events
        .iter()
        .filter_map(CanaryOutcome::from_event)
        .collect();
    assert_eq!(decoded.len(), 4, "one decoded outcome per item");
    for outcome in &report.outcomes {
        let d = decoded
            .iter()
            .find(|d| d.id == outcome.id)
            .unwrap_or_else(|| panic!("outcome {:?} missing from the decoded stream", outcome.id));
        assert_eq!(
            (d.planted, d.verdict_approved),
            (outcome.planted, outcome.verdict_approved),
            "planted and verdict_approved for {:?} must round-trip through the store \
             exactly - these are the two fields the FALSE POSITIVES fold arm reads",
            outcome.id
        );
    }

    // The cross-module fold arm: metrics::project_canary, driven over the REAL recorded
    // events (never a hand-typed fixture), must count all three controls and exactly the
    // two wrongly-rejected ones - the seam neither module's own isolated unit tests (each
    // fixed to one side of it) can catch a disagreement on.
    let m = rigger::metrics::project_canary(&events);
    assert_eq!(
        m.controls, 3,
        "three known-good controls were scored: good-control, bad-control-a, bad-control-b"
    );
    assert_eq!(
        m.control_false_positives, 2,
        "two of the three controls were wrongly rejected: bad-control-a and bad-control-b"
    );
    assert_eq!(
        m.planted, 1,
        "sanity: the fold's planted count is unaffected by the FALSE POSITIVES fold arm"
    );
}
