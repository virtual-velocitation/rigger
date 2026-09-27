//! Periphery (integration) test for spec 61 criterion 4 (LENS FAN-OUT), unit u61c4:
//! `score_item`'s tier-1 lens loop now fans out over `crate::parallel::map_ordered`.
//!
//! Criterion 5 (ITEM SHARDING AND THE JOBS CAP, unit u61c5b) landed after this test was
//! first written and changed WHAT width `run_canary` threads into the lens tier:
//! `run_canary` no longer resolves the lens width itself from
//! `crate::parallel::default_workers()` - it now takes a caller-supplied `jobs` total-
//! concurrent-spawn budget and splits it, via `canary_store::spawn_budget`, between its own outer
//! item-sharding width and this inner lens-fan-out width, so their PRODUCT never exceeds
//! `jobs`. This test drives the PUBLIC entry with the PRODUCTION default
//! (`canary_store::default_jobs()`, never a test-pinned override) exactly as `cmd_canary` does
//! when the operator passes no `--jobs` flag.
//!
//! The implementer's own unit tests pin the fan-out at the PRIVATE `score_item` seam with
//! an explicit, test-chosen worker count (`lenses.len()` or `1`) - a barrier proves the
//! lenses run concurrently and a serial-vs-parallel comparison proves the scored outcome
//! does not depend on the width. Neither drives the PUBLIC entry `run_canary` the CLI
//! (`cmd_canary`) actually calls. A lens width silently dropped on the floor between
//! `run_canary` and `score_item`, or a lens the chunking silently skipped, would pass every
//! existing test and only show up out here, driving the crate's public surface the way the
//! shipped binary does. These per-lens correctness properties do not depend on the EXACT
//! worker count `spawn_budget` resolves to (only that every lens is reached exactly once
//! per item, in the right order) so they hold whether the production budget shards the
//! five lenses over one worker or five.
//!
//! Runs OUTSIDE the crate, over the library's public surface (`rigger::...`), so it also
//! proves `run_canary`, `CanaryItem`, `ReviewPanel`, and `AgentDriver` stay exported and
//! reachable the way an external consumer reaches them - not merely visible to canary.rs's
//! own `mod tests` via `super::` (whose `Scripted` driver and helpers are private to that
//! module and unreachable from here regardless). Neither `canary` nor `parallel` is feature-
//! gated, so this test is compiled unconditionally and runs in both feature lanes.

mod common;

use std::collections::HashSet;
use std::sync::Mutex;

use serde_json::Value;

use common::fixtures::anchor_of;
use common::fixtures::cfg_for;
use common::fixtures::critical_verdict;
use common::fixtures::emit_review_finding;
use common::fixtures::item;
use common::fixtures::panel_with_lenses;
use rigger::canary::{CanaryOutcome, STREAM, TIER_ADVERSARY, TIER_LENS};
use rigger::canary_store::run_canary;
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::AgentDef;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};

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
            return Ok(critical_verdict(prompt));
        }

        // The anchor `review_header` names between the FIRST pair of backticks in the
        // prompt - the same extraction canary.rs's own `mod tests` driver uses, reached
        // independently here since that code is private to canary.rs.
        let anchor = anchor_of(prompt);

        let catches = self
            .catches
            .iter()
            .any(|(id, anc)| *id == a.id && *anc == anchor);
        emit_review_finding(emit, &a.id, &anchor, catches)
    }
}

/// Drives `run_canary` - the public entry the shipped `rigger canary` command calls - over a
/// panel of five lenses, an adversary, and an adjudicator, and a three-item corpus, at the
/// REAL production `--jobs` default (`canary_store::default_jobs()`, never a test-pinned
/// override) - the same budget `cmd_canary` resolves to when the operator names no
/// `--jobs`. Proves:
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
    let cfg = cfg_for(&ids);
    let panel = panel_with_lenses(&lenses);

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
    let report = run_canary(
        &store,
        &driver,
        &cfg,
        &panel,
        &corpus,
        rigger::canary_store::default_jobs(),
        &|_, _| {},
    )
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

    // The production `--jobs` default - never a test-pinned value - is always usable and
    // always greater than one (spec 61's Done-when text for ITEM SHARDING AND THE JOBS
    // CAP), so `run_canary`'s call site always has real budget to shard items and fan the
    // lens tier out with.
    assert!(
        rigger::canary_store::default_jobs() > 1,
        "the default --jobs budget is always greater than one"
    );
}
