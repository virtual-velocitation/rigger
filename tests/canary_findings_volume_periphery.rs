//! Periphery (integration) test for spec 61 criterion 8 (FINDINGS VOLUME), unit u61c8:
//! `CanaryOutcome` gained a `findings_raised: BTreeMap<String, u64>` field - the count of
//! findings each tier RAISED this item, independent of whether any of them CAUGHT the
//! planted defect - and `metrics::project_canary` sums it per tier across every scored item.
//!
//! The implementer's own unit tests pin this at three PRIVATE/internal seams in isolation:
//! `score_item` (canary.rs, hand-picked test driver), `project_canary` (metrics.rs, hand-
//! rolled JSON event fixtures typed by hand to match the wire shape from memory), and
//! `format_canary_stats` (main.rs, a hand-built `CanaryMetrics` never produced by
//! `project_canary` itself). None of the three drives the chain end to end through the
//! PUBLIC production entry `run_canary` the shipped `rigger canary` command calls, so a
//! wire-shape disagreement between `CanaryOutcome::to_event`'s REAL output and
//! `project_canary`'s hand-typed fixture would pass every one of those tests while silently
//! losing the findings-volume measure in a live run. This suite drives `run_canary` with a
//! scripted driver that raises MULTIPLE findings per tier (most of them non-catching noise),
//! reads the events it actually recorded back out of the store, and folds them through the
//! public `metrics::project_canary` - proving the measure survives the real wire round trip
//! and sums correctly across items, not just against fixtures typed to match it.
//!
//! It also proves the WIRE BACK-COMPATIBILITY half of this criterion's surface, from outside
//! the crate: `CanaryOutcome::from_event` is the one public wire-schema authority both the
//! metrics fold and `rigger stats --canary` read through, and a persisted event written by a
//! binary that predates this field (no `findings_raised` key in its JSON payload at all) must
//! still decode successfully rather than erroring or corrupting every OTHER field on the same
//! outcome - a store from before this unit landed does not stop `rigger stats --canary` from
//! working. The implementer's own round-trip test only proves the FORWARD direction (encode a
//! populated map, decode it back); nothing else in the diff proves the pre-existing-data
//! direction.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use rigger::canary::{run_canary, CanaryItem, CanaryOutcome, STREAM, TIER_ADVERSARY, TIER_LENS};
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, ReviewPanel};
use rigger::contextgraph::TYPE_REVIEW_FINDING;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::ledger::TYPE_UNIT_STATUS;

/// The finding summary text that marks a "critical" finding - the exact substring the
/// scripted adjudicator below looks for in its own prompt (which embeds every finding's
/// summary), mirroring how the live adjudicator prompt actually carries findings.
const CRITICAL_SUMMARY: &str = "CRIT defect here";

/// A minimal scripted `AgentDriver` written from scratch for this outside-in layer (it does
/// not, and cannot, reuse canary.rs's own `#[cfg(test)]`-private `Scripted` driver). Every
/// declared agent id raises a FIXED set of findings regardless of which item is under
/// review - lens-a raises one CRITICAL finding naming `"hot.rs"`, lens-b raises one benign
/// noise finding, and the adversary raises two benign noise findings - so the volume per
/// tier is deterministic and known ahead of time (lens: 2, adversary: 2, on every item this
/// panel scores), while whether a tier CATCHES stays governed only by each item's own
/// `anchor` (empty for the control, so `Finding::catches` is unconditionally `false` there
/// regardless of what any finding names) - the two measures are driven independently on
/// purpose, the property this criterion exists to prove.
struct VolumeDriver;

impl AgentDriver for VolumeDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        prompt: &str,
        _opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        match a.id.as_str() {
            "adj" => {
                let reject = prompt.contains(CRITICAL_SUMMARY);
                let verdict = if reject { "reject" } else { "approve" };
                return Ok(AgentResult {
                    output: format!("{{\"verdict\":\"{verdict}\"}}"),
                    resolved_model: String::new(),
                });
            }
            "lens-a" => {
                emit(
                    TYPE_REVIEW_FINDING,
                    json!({"id": "f-lens-a", "by": "lens-a", "summary": CRITICAL_SUMMARY, "about": ["hot.rs"]}),
                )?;
            }
            "lens-b" => {
                emit(
                    TYPE_REVIEW_FINDING,
                    json!({"id": "f-lens-b", "by": "lens-b", "summary": "minor style nit", "about": ["noise-lens.rs"]}),
                )?;
            }
            "adv" => {
                emit(
                    TYPE_REVIEW_FINDING,
                    json!({"id": "f-adv-1", "by": "adv", "summary": "over-flag one", "about": ["noise-adv-1.rs"]}),
                )?;
                emit(
                    TYPE_REVIEW_FINDING,
                    json!({"id": "f-adv-2", "by": "adv", "summary": "over-flag two", "about": ["noise-adv-2.rs"]}),
                )?;
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

fn panel(lenses: &[&str]) -> ReviewPanel {
    ReviewPanel {
        lenses: lenses.iter().map(|s| (*s).to_string()).collect(),
        adversary: "adv".into(),
        adjudicator: "adj".into(),
        tiers: None,
    }
}

fn item(id: &str, anchor: &str, planted: bool, verdict: &str, tier: &str) -> CanaryItem {
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
        expected_tier: tier.into(),
        review: format!("fn {id}() {{}}"),
    }
}

/// Drives `run_canary` - the public entry the shipped `rigger canary` command calls - over a
/// panel whose lenses and adversary each raise MORE findings than they catch, and a
/// two-item corpus: `"hot"` (a planted defect at `hot.rs`, which lens-a's fixed finding
/// names) and `"control"` (a known-good control with an EMPTY anchor, so nothing can ever
/// catch it no matter what any tier names). Proves:
///
///  - `findings_raised` counts the RAW volume a tier raised, not the catch count: on `"hot"`
///    the lens tier catches (lens-a's finding names the anchor) yet still records `2`
///    (lens-a AND lens-b both raised one each), and the adversary tier catches NOTHING yet
///    still records `2` (both its findings are noise) - volume and catch are independent
///    measures on the SAME item, at the real production entry point, not a fixture;
///  - on the control item, where `caught_by` is unconditionally empty (`Finding::catches`
///    short-circuits on an empty anchor), the SAME tiers still record their full raw volume
///    (`2` each) - an over-flagging tier is visible on a known-good item exactly as much as
///    on a planted one, never hidden because nothing was caught;
///  - the recorded outcomes, read back from the store through the SAME
///    [`CanaryOutcome::from_event`] authority `rigger stats --canary` reads through, carry
///    the identical `findings_raised` maps - the measure survives the real wire round trip,
///    not just an in-process return value;
///  - `metrics::project_canary`, driven over those REAL recorded events (never a hand-typed
///    JSON fixture), sums the per-tier volume across BOTH items correctly (2 + 2 = 4 each) -
///    proving the cross-module fold arm from `canary.rs`'s actual wire output into
///    `metrics.rs`'s aggregation, the seam neither module's own unit tests (each fixed to
///    ONE side of it) can catch a disagreement on.
#[test]
fn run_canary_scores_findings_volume_independent_of_catch_and_project_canary_sums_it() {
    let lenses = ["lens-a", "lens-b"];
    let cfg = cfg(&["lens-a", "lens-b", "adv", "adj"]);
    let panel = panel(&lenses);

    let corpus = vec![
        item("hot", "hot.rs", true, "reject", "lens"),
        item("control", "", false, "approve", ""),
    ];

    let store = Store::open(":memory:").expect("an in-memory store opens");
    let report = run_canary(&store, &VolumeDriver, &cfg, &panel, &corpus)
        .expect("run_canary succeeds through the public entry");

    assert_eq!(report.outcomes.len(), 2, "one outcome per corpus item");
    let by_id = |id: &str| -> &CanaryOutcome {
        report
            .outcomes
            .iter()
            .find(|o| o.id == id)
            .unwrap_or_else(|| panic!("no outcome recorded for item {id:?}"))
    };

    let hot = by_id("hot");
    assert_eq!(
        hot.caught_by,
        vec![TIER_LENS.to_string()],
        "lens-a's finding names the anchor, so the lens tier catches this planted defect"
    );
    assert_eq!(
        hot.findings_raised.get(TIER_LENS),
        Some(&2),
        "lens-a AND lens-b each raised one finding - the catching lens's own noisy sibling \
         still counts toward volume, not just the one that caught"
    );
    assert_eq!(
        hot.findings_raised.get(TIER_ADVERSARY),
        Some(&2),
        "the adversary raised two findings on this item and caught neither - volume is \
         counted regardless"
    );

    let control = by_id("control");
    assert!(
        control.caught_by.is_empty(),
        "an empty anchor can never be caught, no matter what any tier's findings name"
    );
    assert_eq!(
        control.findings_raised.get(TIER_LENS),
        Some(&2),
        "the SAME two lenses still raise their fixed findings on the control item - an \
         over-flagging tier is visible on a known-good item too, not masked by zero catches"
    );
    assert_eq!(
        control.findings_raised.get(TIER_ADVERSARY),
        Some(&2),
        "the adversary's over-flagging is visible on the control item as well"
    );

    // The production write path: read the canary stream back through the real store and
    // decode it with the SAME wire-schema authority `rigger stats --canary` reads through,
    // proving `findings_raised` specifically - not just the struct's other fields - round-
    // trips byte-for-byte through the store, not merely the in-process return value.
    let events = store
        .read_stream(STREAM, 0, Direction::Forward)
        .expect("the canary stream reads back");
    let decoded: Vec<CanaryOutcome> = events
        .iter()
        .filter_map(CanaryOutcome::from_event)
        .collect();
    assert_eq!(decoded.len(), 2, "one decoded outcome per item");
    for outcome in &report.outcomes {
        let d = decoded
            .iter()
            .find(|d| d.id == outcome.id)
            .unwrap_or_else(|| panic!("outcome {:?} missing from the decoded stream", outcome.id));
        assert_eq!(
            d.findings_raised, outcome.findings_raised,
            "findings_raised for {:?} must round-trip through the store exactly",
            outcome.id
        );
    }

    // The cross-module fold arm: metrics::project_canary, driven over the REAL recorded
    // events (never a hand-typed fixture), must sum the per-tier volume across both items -
    // proving canary.rs's actual wire output and metrics.rs's fold agree on the shape, a
    // disagreement neither module's own isolated unit tests could see.
    let m = rigger::metrics::project_canary(&events);
    assert_eq!(
        m.findings_raised.get(TIER_LENS),
        Some(&4),
        "2 (hot) + 2 (control) summed across both items via the real fold"
    );
    assert_eq!(
        m.findings_raised.get(TIER_ADVERSARY),
        Some(&4),
        "2 (hot) + 2 (control) summed across both items via the real fold"
    );
}

/// [`CanaryOutcome::from_event`] is the ONE public wire-schema authority both the metrics
/// fold and `rigger stats --canary` read a canary outcome through. A store that predates
/// this unit holds outcome events whose JSON payload has no `findings_raised` key at all -
/// the implementer's own round-trip test only proves the FORWARD direction (encode a
/// populated map, decode the same shape back), never this pre-existing-data direction. This
/// test hand-builds an event with EXACTLY the pre-u61c8 wire shape (every field this type
/// carried before, and nothing else) and proves it still decodes: `findings_raised` reads an
/// honest empty map rather than the decode failing or any OTHER field on the same outcome
/// coming back wrong - a store from before this field existed does not stop `rigger stats
/// --canary` from reading a run recorded under the old binary.
#[test]
fn canary_outcome_from_event_decodes_a_pre_existing_wire_payload_with_no_findings_raised_key() {
    let legacy_payload = json!({
        "id": "legacy-item",
        "status": "canary",
        "defect_class": "off-by-one",
        "planted": true,
        "expected_reject": true,
        "expected_tier": "lens",
        "caught_by": ["lens"],
        "verdict_approved": false,
        "verdict_correct": true,
        "stable": true,
        // deliberately no "findings_raised" key - the pre-u61c8 wire shape.
    });
    let event = Event::new(
        TYPE_UNIT_STATUS,
        serde_json::to_vec(&legacy_payload).unwrap(),
    );

    let decoded = CanaryOutcome::from_event(&event)
        .expect("an event predating this field must still decode, not return None");

    assert_eq!(
        decoded.findings_raised,
        BTreeMap::new(),
        "a wire payload with no findings_raised key decodes to an honest empty map"
    );
    // Every OTHER field on the same outcome must still come back correctly - the missing
    // key must not silently corrupt or truncate decoding of the rest of the struct.
    assert_eq!(decoded.id, "legacy-item");
    assert_eq!(decoded.defect_class, "off-by-one");
    assert!(decoded.planted);
    assert!(decoded.expected_reject);
    assert_eq!(decoded.expected_tier, "lens");
    assert_eq!(decoded.caught_by, vec![TIER_LENS.to_string()]);
    assert!(!decoded.verdict_approved);
    assert!(decoded.verdict_correct);
    assert!(decoded.stable);
}
