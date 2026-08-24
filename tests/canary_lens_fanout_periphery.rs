//! Periphery (integration) test for spec 61 criterion 4 (LENS FAN-OUT), unit u61c4:
//! `score_item`'s tier-1 lens loop now fans out over `crate::parallel::map_ordered`, and
//! `run_canary`'s production call site threads the REAL `crate::parallel::default_workers()`
//! width into it.
//!
//! The implementer's own unit tests pin this at the PRIVATE `score_item` seam with an
//! explicit, test-chosen worker count (`lenses.len()` or `1`) - a barrier proves the lenses
//! run concurrently and a serial-vs-parallel comparison proves the scored outcome does not
//! depend on the width. Neither drives the PUBLIC entry `run_canary` the CLI (`cmd_canary`)
//! actually calls, and neither uses the width production actually resolves -
//! `crate::parallel::default_workers()`, which `run_canary` never lets a caller override. A
//! width silently dropped on the floor between `run_canary` and `score_item`, or a lens the
//! chunking silently skipped, would pass every existing test and only show up out here,
//! driving the crate's public surface the way the shipped binary does.
//!
//! Runs OUTSIDE the crate, over the library's public surface (`rigger::...`), so it also
//! proves `run_canary`, `CanaryItem`, `ReviewPanel`, and `AgentDriver` stay exported and
//! reachable the way an external consumer reaches them - not merely visible to canary.rs's
//! own `mod tests` via `super::` (whose `Scripted` driver and helpers are private to that
//! module and unreachable from here regardless). Neither `canary` nor `parallel` is feature-
//! gated, so this test is compiled unconditionally and runs in both feature lanes.

use std::collections::HashSet;
use std::sync::Mutex;

use serde_json::{json, Value};

use rigger::canary::{run_canary, CanaryItem, CanaryOutcome, STREAM, TIER_ADVERSARY, TIER_LENS};
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, ReviewPanel};
use rigger::contextgraph::TYPE_REVIEW_FINDING;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};

/// The finding summary text that marks a "critical" finding - the exact substring the
/// adjudicator half of [`RecordingDriver`] looks for in its own prompt (which embeds every
/// finding's summary), mirroring how the live adjudicator prompt actually carries findings.
const CRITICAL_SUMMARY: &str = "CRIT defect here";

/// A minimal scripted `AgentDriver`, written FROM SCRATCH for this outside-in layer (it does
/// not, and cannot, reuse canary.rs's own `#[cfg(test)]`-private `Scripted` driver). A
/// reviewer (lens or adversary) raises a finding about the item's anchor file only when its
/// `(agent id, anchor)` pair is listed in `catches`; the adjudicator rejects iff any finding
/// it was shown carries [`CRITICAL_SUMMARY`]. Every spawned agent id is recorded in `seen`
/// under a mutex, so the fan-out's real concurrent calls (whatever width the host resolves)
/// can safely record from multiple threads at once.
struct RecordingDriver {
    catches: Vec<(&'static str, &'static str)>,
    seen: Mutex<Vec<String>>,
}

impl AgentDriver for RecordingDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        prompt: &str,
        _opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        self.seen.lock().unwrap().push(a.id.clone());

        if a.id == "adj" {
            let reject = prompt.contains(CRITICAL_SUMMARY);
            let verdict = if reject { "reject" } else { "approve" };
            return Ok(AgentResult {
                output: format!("{{\"verdict\":\"{verdict}\"}}"),
                resolved_model: String::new(),
            });
        }

        // The anchor `review_header` names between the FIRST pair of backticks in the
        // prompt - the same extraction canary.rs's own `mod tests` driver uses, reached
        // independently here since that code is private to canary.rs.
        let anchor = prompt
            .split_once('`')
            .and_then(|(_, rest)| rest.split_once('`'))
            .map(|(anchor, _)| anchor.to_string())
            .unwrap_or_default();

        let catches = self
            .catches
            .iter()
            .any(|(id, anc)| *id == a.id && *anc == anchor);
        let finding = if catches {
            json!({"id": format!("f-{}", a.id), "by": a.id, "summary": CRITICAL_SUMMARY, "about": [anchor]})
        } else {
            json!({"id": format!("f-{}", a.id), "by": a.id, "summary": "minor style nit", "about": ["other.rs"]})
        };
        emit(TYPE_REVIEW_FINDING, finding)?;
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

fn item(id: &str, defect_class: &str, planted: bool, verdict: &str, tier: &str) -> CanaryItem {
    CanaryItem {
        id: id.into(),
        defect_class: defect_class.into(),
        planted,
        anchor: format!("{id}.rs"),
        expected_verdict: verdict.into(),
        expected_tier: tier.into(),
        review: format!("fn {id}() {{}}"),
    }
}

/// Drives `run_canary` - the public entry the shipped `rigger canary` command calls - over a
/// panel of five lenses, an adversary, and an adjudicator, and a three-item corpus, at the
/// REAL `crate::parallel::default_workers()` width (never overridden by a test). Proves:
///
///  - a catch attributed to a LENS OTHER THAN THE FIRST (`lens-c`, the middle of five) still
///    scores correctly - the property the fan-out's chunked, index-preserving aggregation
///    exists to preserve, and the one a bug that dropped or misordered a worker's chunk
///    would break silently for anyone whose catching lens is not the first;
///  - every declared lens is actually spawned once per item - a lens the chunking silently
///    skipped would still leave `caught_by` correct for items no OTHER lens catches, so this
///    checks spawn counts directly rather than inferring reach from outcomes alone;
///  - the adversary tier (unchanged by this diff, sequential after the now-parallel lens
///    tier) still catches what the lenses miss;
///  - a known-good control still approves with an empty `caught_by`;
///  - the scored outcomes the call returns are exactly what the store recorded - the
///    production entry's real write path, decoded with the same [`CanaryOutcome::from_event`]
///    authority `rigger stats --canary` reads through - round-trips for a run whose lens tier
///    fanned out, not just the in-process return value.
#[test]
fn run_canary_fans_out_the_lens_tier_at_the_real_default_width_through_the_public_entry() {
    let lenses = ["lens-a", "lens-b", "lens-c", "lens-d", "lens-e"];
    let mut ids: Vec<&str> = lenses.to_vec();
    ids.extend(["adv", "adj"]);
    let cfg = cfg(&ids);
    let panel = panel(&lenses);

    let corpus = vec![
        item("mid-lens", "off-by-one", true, "reject", "lens"),
        item(
            "adversary-catch",
            "resource-leak",
            true,
            "reject",
            "adversary",
        ),
        item("clean", "none", false, "approve", ""),
    ];

    let driver = RecordingDriver {
        // lens-c sits at index 2 of 5 - neither first nor last - so a fan-out that
        // silently dropped or misordered any chunk but the first would still be caught.
        catches: vec![("lens-c", "mid-lens.rs"), ("adv", "adversary-catch.rs")],
        seen: Mutex::new(Vec::new()),
    };

    let store = Store::open(":memory:").expect("an in-memory store opens");
    let report = run_canary(&store, &driver, &cfg, &panel, &corpus)
        .expect("run_canary succeeds through the public entry");

    assert_eq!(report.outcomes.len(), 3, "one outcome per corpus item");
    let by_id = |id: &str| -> &CanaryOutcome {
        report
            .outcomes
            .iter()
            .find(|o| o.id == id)
            .unwrap_or_else(|| panic!("no outcome recorded for item {id:?}"))
    };

    let mid = by_id("mid-lens");
    assert_eq!(
        mid.caught_by,
        vec![TIER_LENS.to_string()],
        "a catch by a non-first lens (lens-c, index 2 of 5) still attributes to the lens tier"
    );
    assert!(
        !mid.verdict_approved,
        "a planted defect the lens tier caught is rejected"
    );
    assert!(mid.verdict_correct);

    let adv = by_id("adversary-catch");
    assert_eq!(
        adv.caught_by,
        vec![TIER_ADVERSARY.to_string()],
        "the adversary tier, sequential after the now-parallel lens tier, still catches"
    );
    assert!(!adv.verdict_approved);
    assert!(adv.verdict_correct);

    let clean = by_id("clean");
    assert!(
        clean.caught_by.is_empty(),
        "a known-good control catches nothing"
    );
    assert!(clean.verdict_approved, "a known-good control is approved");
    assert!(clean.verdict_correct);

    // Every declared lens was actually reached, once per corpus item - proof the fan-out's
    // chunk aggregation did not silently drop a worker's range. A dropped lens-c would still
    // leave "mid-lens" uncaught (already asserted above); this checks reach directly.
    let seen = driver.seen.into_inner().unwrap();
    let distinct_lenses_seen: HashSet<&str> = seen
        .iter()
        .map(String::as_str)
        .filter(|id| lenses.contains(id))
        .collect();
    assert_eq!(
        distinct_lenses_seen.len(),
        lenses.len(),
        "every declared lens must be spawned; got {seen:?}"
    );
    for lens in lenses {
        let count = seen.iter().filter(|id| id.as_str() == lens).count();
        assert_eq!(
            count,
            corpus.len(),
            "lens {lens} must be spawned once per corpus item ({} items); got {count}",
            corpus.len()
        );
    }

    // The production write path: read the canary stream back through the real store and
    // decode it with the SAME wire-schema authority `rigger stats --canary` reads through,
    // proving the recorded events - not just the in-process return value - are exactly the
    // scored outcomes, for a run whose lens tier fanned out.
    let events = store
        .read_stream(STREAM, 0, Direction::Forward)
        .expect("the canary stream reads back");
    let decoded: Vec<CanaryOutcome> = events
        .iter()
        .filter_map(CanaryOutcome::from_event)
        .collect();
    assert_eq!(
        decoded.len(),
        3,
        "one decoded outcome per item (the batch marker itself decodes to None)"
    );
    for outcome in &report.outcomes {
        assert!(
            decoded.iter().any(|d| d == outcome),
            "outcome {:?} round-trips through the store byte-for-byte",
            outcome.id
        );
    }

    // The width production actually resolves - never a test-pinned value - is always
    // usable; `run_canary`'s call site has nothing to fan the lens tier over otherwise.
    assert!(
        rigger::parallel::default_workers() >= 1,
        "the default lens-tier width is never zero"
    );
}
