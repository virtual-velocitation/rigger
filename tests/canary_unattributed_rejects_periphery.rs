//! Periphery (integration) test for spec 61 criterion 2 (NO FAKE ZEROS), unit u61c2b:
//! `metrics::CanaryMetrics` gained an `unattributed_correct_rejects: u64` field -
//! `project_canary`'s fold now counts a planted item the adjudicator correctly rejected
//! whose `caught_by` came back empty, distinct from a tier that looked and genuinely
//! caught nothing - and `format_canary_stats` reads that count to print `n/a` instead of
//! a fake `0/N (0.0%)`.
//!
//! The implementer's own unit tests pin this at two PRIVATE/internal seams in isolation:
//! `project_canary` (metrics.rs, driven over a hand-rolled JSON event fixture typed by
//! hand to match the wire shape from memory via a local `canary_outcome` helper) and
//! `format_canary_stats` (main.rs, driven against a hand-built `CanaryMetrics` that was
//! never produced by `project_canary` itself). Neither drives the chain through the
//! PUBLIC production entry `run_canary` the shipped `rigger canary` command calls, so a
//! wire-shape disagreement between `CanaryOutcome::to_event`'s REAL output and either
//! fixture would pass both tests while silently miscounting `unattributed_correct_rejects`
//! in a live run.
//!
//! This suite drives `run_canary` with a scripted panel whose single lens's behavior is
//! routed by a marker embedded in each corpus item's review body (the only per-item
//! signal a driver receives), decoupling two measures that must vary INDEPENDENTLY for
//! this criterion to mean anything: whether the adjudicator's verdict is correct, and
//! whether any tier's `about` field happened to name the item's anchor. It then reads the
//! events actually recorded back out of the store, decodes them through the same
//! `CanaryOutcome::from_event` wire authority `rigger stats --canary` reads through, and
//! folds them via the public `metrics::project_canary` - proving the count survives the
//! real wire round trip over four independently-varying items, not just a fixture typed
//! to match it.

use rigger::canary::{default_jobs, run_canary, CanaryItem, CanaryOutcome, STREAM, TIER_LENS};
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, ReviewPanel};
use rigger::contextgraph::TYPE_REVIEW_FINDING;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use serde_json::{json, Value};

/// The text this driver's scripted adjudicator looks for verbatim in its own prompt
/// (which embeds every finding's summary) to decide whether to reject - mirroring how a
/// live adjudicator's reject decision is driven by what it actually reads, never by
/// whether some finding's `about` field happens to name the anchor. This is the exact
/// independence the NO FAKE ZEROS criterion exists to measure: a panel can correctly
/// reject an item for a reason no tier's attribution captures.
const REJECT_MARKER: &str = "REJECT-THIS-ITEM";

/// A minimal scripted `AgentDriver` written from scratch for this outside-in layer (it
/// does not, and cannot, reuse canary.rs's own `#[cfg(test)]`-private driver). The single
/// lens's behavior is routed by a marker embedded in the item's review body under test -
/// the only per-item signal a driver receives via the prompt - rather than by agent id, so
/// one lens can raise a reject-worthy finding that either DOES or DOES NOT name the item's
/// anchor.
struct UnattributedDriver;

impl AgentDriver for UnattributedDriver {
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
                if prompt.contains("SILENT-MARKER") {
                    // Reject-worthy, but names a file OTHER than this item's anchor: the
                    // adjudicator correctly rejects, yet no tier's attribution matches.
                    emit(
                        TYPE_REVIEW_FINDING,
                        json!({
                            "id": "f-silent",
                            "by": "lens",
                            "summary": format!("{REJECT_MARKER}: something is wrong"),
                            "about": ["decoy-file.rs"],
                        }),
                    )?;
                } else if prompt.contains("ATTRIBUTED-MARKER") {
                    // Reject-worthy AND names the real anchor: both the verdict and the
                    // attribution are captured on this sibling item.
                    emit(
                        TYPE_REVIEW_FINDING,
                        json!({
                            "id": "f-attributed",
                            "by": "lens",
                            "summary": format!("{REJECT_MARKER}: also wrong"),
                            "about": ["attributed-secret.rs"],
                        }),
                    )?;
                }
                // MISSED-MARKER and the control item: the lens raises nothing at all.
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

/// Drives `run_canary` over four independently-varying planted/control items and proves
/// `metrics::project_canary`'s new fold arm counts EXACTLY the one item that is both
/// correctly rejected and unattributed - never the attributed sibling (real catch), the
/// genuine miss (wrong verdict despite empty attribution), or the known-good control (not
/// planted, so it never reaches the fold arm at all) - over real wire data recorded and
/// read back through the actual store, not a hand-typed fixture.
#[test]
fn run_canary_scores_an_unattributed_correct_reject_and_project_canary_counts_only_it() {
    let cfg = cfg(&["lens", "adj"]);
    let panel = panel();

    let corpus = vec![
        // Correctly rejected; the reject-worthy finding names a DIFFERENT file than the
        // anchor - attribution is never captured. THE case this criterion exists for.
        item(
            "silent",
            "silent-secret.rs",
            true,
            "reject",
            "SILENT-MARKER",
        ),
        // Correctly rejected AND attributed - a real catch, not a fake zero. Excluded.
        item(
            "attributed",
            "attributed-secret.rs",
            true,
            "reject",
            "ATTRIBUTED-MARKER",
        ),
        // Nothing raised at all: the adjudicator sees no reject marker and wrongly
        // approves a planted defect - a genuine miss, not an unmeasured catch. Excluded.
        item(
            "missed",
            "missed-secret.rs",
            true,
            "reject",
            "MISSED-MARKER",
        ),
        // Known-good control: correctly approved, empty attribution, but not planted at
        // all - there is nothing to catch, so it never reaches the fold arm. Excluded.
        item("control", "", false, "approve", "CONTROL-MARKER"),
    ];

    let store = Store::open(":memory:").expect("an in-memory store opens");
    // This suite pins the NO FAKE ZEROS wire round trip, not the ITEM SHARDING/LENS
    // FAN-OUT jobs-cap concurrency itself (already pinned by run_canary's own
    // `run_canary_scores_identically_regardless_of_the_jobs_width`), so it takes the
    // ordinary default budget rather than asserting a specific width.
    let report = run_canary(
        &store,
        &UnattributedDriver,
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

    let silent = by_id("silent");
    assert!(
        silent.verdict_correct,
        "the adjudicator correctly rejects: it saw the reject marker in the finding \
         raised about this item"
    );
    assert!(
        silent.caught_by.is_empty(),
        "the finding named a different file than this item's anchor, so no tier's \
         attribution matched - the verdict was right for a reason attribution missed"
    );

    let attributed = by_id("attributed");
    assert!(attributed.verdict_correct, "also correctly rejected");
    assert_eq!(
        attributed.caught_by,
        vec![TIER_LENS.to_string()],
        "this finding named the real anchor, so the lens tier's attribution matched - a \
         REAL catch, the sibling case that must not be swept into the unattributed count"
    );

    let missed = by_id("missed");
    assert!(
        !missed.verdict_correct,
        "no finding was raised at all, so the adjudicator saw no reject marker and wrongly \
         approved a planted defect - a genuine miss, not an unmeasured catch"
    );
    assert!(missed.caught_by.is_empty());

    let control = by_id("control");
    assert!(
        control.verdict_correct && control.caught_by.is_empty(),
        "a known-good control is correctly approved with nothing to attribute"
    );

    // Read the events back through the real store and decode with the SAME wire-schema
    // authority `rigger stats --canary` reads through - proving the fold below runs over
    // genuine recorded wire data, not an in-process return value or a fixture typed by
    // hand to match it.
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
            (d.verdict_correct, &d.caught_by),
            (outcome.verdict_correct, &outcome.caught_by),
            "verdict_correct and caught_by for {:?} must round-trip through the store \
             exactly - these are the two fields the fold arm reads",
            outcome.id
        );
    }

    // The cross-module fold arm: metrics::project_canary, driven over the REAL recorded
    // events (never a hand-typed fixture), must count exactly the one correctly-rejected
    // item whose attribution was never captured - the seam neither module's own isolated
    // unit tests (each fixed to one side of it) can catch a disagreement on.
    let m = rigger::metrics::project_canary(&events);
    assert_eq!(
        m.unattributed_correct_rejects, 1,
        "only 'silent' is a correctly-rejected planted item with empty attribution; \
         'attributed' has a real catch, 'missed' has the wrong verdict, and 'control' is \
         not planted"
    );
}
