//! Periphery (integration) test for spec 61 criterion 5 (ITEM SHARDING AND THE JOBS CAP),
//! unit u61c5b: `run_canary` gained a caller-supplied `jobs: usize` total-concurrent-spawn
//! budget, and a new private `canary::spawn_budget` splits it between the function's own
//! OUTER per-item sharding (new: `run_canary` now calls `crate::parallel::map_ordered`
//! itself, a call site that did not exist before this unit) and the LENS FAN-OUT
//! criterion's already-built INNER `score_item` fan-out, so their PRODUCT never exceeds
//! `jobs`. A new `canary::default_jobs()` supplies the production default when the operator
//! (or a direct library caller) does not choose one.
//!
//! The implementer's own unit tests pin every property above at the PRIVATE `canary.rs`
//! seam, with a `#[cfg(test)]`-private `Scripted`/`BarrierGatedEverySpawn` driver
//! unreachable from here. This suite re-proves the same behavioral contracts from OUTSIDE
//! the crate, over the library's public surface (`rigger::canary::run_canary`,
//! `rigger::canary::default_jobs`), using drivers built fresh in this file (the same
//! "written FROM SCRATCH, cannot reuse the private one" discipline
//! `tests/canary_lens_fanout_periphery.rs` already established for criterion 4) - so a
//! wiring bug between the public entry and the private budget-splitting internals (e.g. a
//! refactor that silently stopped threading `jobs` through, or reverted the outer loop to
//! serial) would be caught here even if the implementer's own tests were adjusted alongside
//! it, since these tests never see canary.rs's internals at all.
//!
//! Also covers a contract NOT exercised anywhere else: the new outer `map_ordered` seam's
//! run-to-completion-on-error behavior interacting with `run_canary`'s aggregation loop -
//! every item is scored regardless of a sibling's error, but only items BEFORE the first
//! erroring one (in corpus order) are ever appended to the store, matching this crate's own
//! "a canary score the store did not write is not a score" discipline.

use std::sync::{Barrier, Mutex};

use serde_json::{json, Value};

use rigger::canary::{run_canary, CanaryItem, CanaryOutcome, STREAM, TIER_LENS};
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, ReviewPanel};
use rigger::contextgraph::TYPE_REVIEW_FINDING;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};

const CRITICAL_SUMMARY: &str = "CRIT defect here";

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

fn item(id: &str, planted: bool, verdict: &str, tier: &str) -> CanaryItem {
    CanaryItem {
        id: id.into(),
        defect_class: if planted {
            "off-by-one".into()
        } else {
            "none".into()
        },
        planted,
        anchor: format!("{id}.rs"),
        expected_verdict: verdict.into(),
        expected_tier: tier.into(),
        review: format!("fn {id}() {{}}"),
    }
}

/// Extract the anchor a reviewer prompt names - the file between the FIRST pair of
/// backticks `review_header` wraps it in. Re-derived here rather than shared, since this
/// file cannot see canary.rs's private helper either.
fn anchor_of(prompt: &str) -> String {
    prompt
        .split_once('`')
        .and_then(|(_, rest)| rest.split_once('`'))
        .map(|(anchor, _)| anchor.to_string())
        .unwrap_or_default()
}

/// `default_jobs()` is not merely "greater than one" (already pinned in
/// `canary_lens_fanout_periphery.rs`) but an EXACT, reproducible formula: the crate-wide
/// default worker width, floored at 2. Pinning the formula (not just the inequality) at the
/// public boundary means a future change that weakens the floor (e.g. `.max(1)`) or drops
/// the crate-wide-width reuse (hand-rolling a second width authority) fails here even on a
/// multi-core host where the inequality alone would still hold.
#[test]
fn default_jobs_equals_default_workers_floored_at_two() {
    assert_eq!(
        rigger::canary::default_jobs(),
        rigger::parallel::default_workers().max(2),
        "default_jobs must be exactly default_workers() floored at 2, not merely > 1"
    );
}

/// `jobs=0`, a value the CLI never lets an operator pass (`--jobs` is validated `> 0`
/// before it reaches the library) but a direct library caller legally can, since `jobs` is
/// a plain `usize`. Proves the public entry never panics and never hangs on a zero-width
/// pool - it degrades to the fully serial walk, per `spawn_budget`'s documented `jobs == 0
/// -> width 1` contract, observed here purely through `run_canary`'s return value (the
/// private `spawn_budget` itself is unreachable from this file).
#[test]
fn run_canary_with_a_zero_jobs_budget_degrades_to_a_serial_width_without_panicking() {
    let ids = ["lens-a", "adv", "adj"];
    let c = cfg(&ids);
    let p = panel(&["lens-a"]);
    let corpus = vec![item("only", true, "reject", "lens")];

    struct Catches;
    impl AgentDriver for Catches {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            _opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id == "adj" {
                let reject = prompt.contains(CRITICAL_SUMMARY);
                let verdict = if reject { "reject" } else { "approve" };
                return Ok(AgentResult {
                    output: format!("{{\"verdict\":\"{verdict}\"}}"),
                    resolved_model: String::new(),
                });
            }
            // Only the lens tier catches here - the adversary stays clean, so the
            // resulting caught_by names exactly the lens tier, not both.
            let finding = if a.id == "lens-a" {
                json!({
                    "id": format!("f-{}", a.id),
                    "by": a.id,
                    "summary": CRITICAL_SUMMARY,
                    "about": [anchor_of(prompt)],
                })
            } else {
                json!({
                    "id": format!("f-{}", a.id),
                    "by": a.id,
                    "summary": "minor style nit",
                    "about": ["other.rs"],
                })
            };
            emit(TYPE_REVIEW_FINDING, finding)?;
            Ok(AgentResult {
                output: "reviewed".into(),
                resolved_model: String::new(),
            })
        }
    }

    let store = Store::open(":memory:").expect("an in-memory store opens");
    let report = run_canary(&store, &Catches, &c, &p, &corpus, 0)
        .expect("jobs=0 must not panic or error - it degrades to a serial width");
    assert_eq!(report.outcomes.len(), 1);
    assert_eq!(report.outcomes[0].caught_by, vec![TIER_LENS.to_string()]);
    assert!(report.outcomes[0].verdict_correct);
}

/// The core wiring contract: a small, caller-chosen `jobs` value genuinely bounds the
/// TOTAL number of review-panel spawns in flight at once, summed ACROSS every concurrently
/// sharded item's lens fan-out together - not each dimension independently reusing the
/// same number. Three items x two lenses, `jobs=6` (`spawn_budget(6,3) = (3,2)`, an exact
/// fit): every lens spawn across the WHOLE run blocks on a barrier sized to exactly
/// `3*2=6`. This can only pass if item sharding (3 concurrent items) and lens fan-out (2
/// concurrent lenses per item) are truly COMBINED at the same instant - if either dimension
/// were silently serialized, or the two dimensions were each bounded by `jobs` independently
/// (rather than their product), fewer than 6 lens spawns would ever be in flight
/// simultaneously and this test would hang.
///
/// A driver written fresh for this file (never reusing canary.rs's private
/// `#[cfg(test)]`-only `Scripted`/`BarrierGatedEverySpawn`) also proves the scored outcome
/// stays CORRECT under that real concurrency: a catch attributed to `lens-b` (not the first
/// lens) on the middle item still attributes to the lens tier.
#[test]
fn run_canary_jobs_budget_bounds_total_concurrent_spawns_through_the_public_entry() {
    let lenses = ["lens-a", "lens-b"];
    let mut ids: Vec<&str> = lenses.to_vec();
    ids.extend(["adv", "adj"]);
    let c = cfg(&ids);
    let p = panel(&lenses);
    let corpus = vec![
        item("i1", false, "approve", ""),
        item("i2", true, "reject", "lens"),
        item("i3", false, "approve", ""),
    ];

    struct BarrierGatedLenses {
        barrier: Barrier,
    }
    impl AgentDriver for BarrierGatedLenses {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            _opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id == "adj" {
                let reject = prompt.contains(CRITICAL_SUMMARY);
                let verdict = if reject { "reject" } else { "approve" };
                return Ok(AgentResult {
                    output: format!("{{\"verdict\":\"{verdict}\"}}"),
                    resolved_model: String::new(),
                });
            }
            // Only the lens tier is sharded by this unit's `item_workers x lens_workers`
            // product; the adversary runs once per item, sequential after the lens tier -
            // never enough calls in flight to reach a barrier of 6 on its own, so it must
            // stay outside the wait (mirroring the implementer's own analogous unit test).
            if a.id != "adv" {
                self.barrier.wait();
            }
            let anchor = anchor_of(prompt);
            let catches = a.id == "lens-b" && anchor == "i2.rs";
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

    let driver = BarrierGatedLenses {
        barrier: Barrier::new(6),
    };
    let store = Store::open(":memory:").expect("an in-memory store opens");
    let report = run_canary(&store, &driver, &c, &p, &corpus, 6)
        .expect("run_canary succeeds through the public entry at jobs=6");

    assert_eq!(report.outcomes.len(), 3, "one outcome per corpus item");
    let by_id = |id: &str| -> &CanaryOutcome {
        report
            .outcomes
            .iter()
            .find(|o| o.id == id)
            .unwrap_or_else(|| panic!("no outcome recorded for item {id:?}"))
    };
    assert_eq!(
        by_id("i2").caught_by,
        vec![TIER_LENS.to_string()],
        "a catch by lens-b (not the first lens) on the middle item still scores correctly \
         under the combined item x lens concurrency"
    );
    assert!(by_id("i1").caught_by.is_empty());
    assert!(by_id("i1").verdict_approved);
    assert!(by_id("i3").caught_by.is_empty());
    assert!(by_id("i3").verdict_approved);
}

/// The determinism constraint the `--jobs` knob must never break: a caller who picks a
/// different `jobs` value gets the SAME scored outcomes, in the SAME order, as the fully
/// serial walk. `jobs=1` forces `spawn_budget(1, n) = (1, 1)` (serial on both dimensions);
/// `jobs=12` engages real concurrency on both. Proven through the public entry with a
/// driver built fresh for this file (no shared mutable state needed - purely a function of
/// each prompt's content - so the comparison cannot be confounded by scheduling order).
#[test]
fn run_canary_scores_identically_regardless_of_jobs_width_through_the_public_entry() {
    let lenses = ["lens-a", "lens-b", "lens-c"];
    let mut ids: Vec<&str> = lenses.to_vec();
    ids.extend(["adv", "adj"]);
    let c = cfg(&ids);
    let p = panel(&lenses);
    let corpus = vec![
        item("i1", true, "reject", "lens"),
        item("i2", true, "reject", "adversary"),
        item("i3", false, "approve", ""),
        item("i4", true, "reject", "lens"),
    ];

    struct Deterministic;
    impl AgentDriver for Deterministic {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            _opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id == "adj" {
                let reject = prompt.contains(CRITICAL_SUMMARY);
                let verdict = if reject { "reject" } else { "approve" };
                return Ok(AgentResult {
                    output: format!("{{\"verdict\":\"{verdict}\"}}"),
                    resolved_model: String::new(),
                });
            }
            let anchor = anchor_of(prompt);
            // lens-c catches i1/i4; the adversary catches i2; nobody catches i3 (control).
            let catches = (a.id == "lens-c" && (anchor == "i1.rs" || anchor == "i4.rs"))
                || (a.id == "adv" && anchor == "i2.rs");
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

    let driver = Deterministic;
    let serial_store = Store::open(":memory:").expect("an in-memory store opens");
    let serial = run_canary(&serial_store, &driver, &c, &p, &corpus, 1)
        .expect("the serial (jobs=1) walk succeeds");
    let parallel_store = Store::open(":memory:").expect("an in-memory store opens");
    let parallel = run_canary(&parallel_store, &driver, &c, &p, &corpus, 12)
        .expect("the sharded/fanned-out (jobs=12) walk succeeds");

    assert_eq!(
        serial.outcomes, parallel.outcomes,
        "sharded item scoring at a generous jobs width must match the serial walk exactly, \
         in the same corpus order"
    );
}

/// The new outer `map_ordered` seam's most consequential behavioral change from the serial
/// for-loop it replaced: EVERY item is scored (spawned) to completion, even after another
/// item's spawn has already failed - `map_ordered` has no early-return, so a sibling item's
/// error cannot stop an in-flight item's spawn. Proven by tracking every item actually
/// reaching the driver in a shared, mutex-guarded log while the MIDDLE item (`i2`) errors.
///
/// Also pins the aggregation loop's own contract, which run-to-completion alone does not
/// guarantee: `run_canary` still walks `scored` in fixed CORPUS order and returns the FIRST
/// error by that order via `?`, so an item AFTER the erroring one (`i3`, fully computed by
/// `map_ordered` before this loop ever runs) is silently discarded - never appended to the
/// store - while an item BEFORE it (`i1`) is already durable. This crate's own discipline
/// is "a canary score the store did not write is not a score"
/// (`the_canary_records_nothing_it_cannot_find_afterwards`); this test is the periphery
/// proof that a run which ultimately errors can still leave a PARTIAL, non-atomic trace in
/// the store - a caller retrying after fixing the failing item must not assume a clean
/// slate.
#[test]
fn run_canary_runs_every_item_to_completion_even_when_one_items_spawn_errors() {
    let ids = ["lens-a", "adv", "adj"];
    let c = cfg(&ids);
    let p = panel(&["lens-a"]);
    let corpus = vec![
        item("i1", false, "approve", ""),
        item("i2", true, "reject", "lens"),
        item("i3", false, "approve", ""),
    ];

    struct ErrorsOnOneItem {
        attempted: Mutex<Vec<String>>,
    }
    impl AgentDriver for ErrorsOnOneItem {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            _opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id == "adj" {
                return Ok(AgentResult {
                    output: "{\"verdict\":\"approve\"}".into(),
                    resolved_model: String::new(),
                });
            }
            let anchor = anchor_of(prompt);
            self.attempted.lock().unwrap().push(anchor.clone());
            if anchor == "i2.rs" {
                return Err(Error("simulated reviewer failure".to_string()));
            }
            let finding = json!({
                "id": format!("f-{}", a.id),
                "by": a.id,
                "summary": "minor style nit",
                "about": ["other.rs"],
            });
            emit(TYPE_REVIEW_FINDING, finding)?;
            Ok(AgentResult {
                output: "reviewed".into(),
                resolved_model: String::new(),
            })
        }
    }

    let driver = ErrorsOnOneItem {
        attempted: Mutex::new(Vec::new()),
    };
    let store = Store::open(":memory:").expect("an in-memory store opens");
    // jobs=3 -> spawn_budget(3, 3) = (3, 1): all three items shard concurrently.
    let result = run_canary(&store, &driver, &c, &p, &corpus, 3);
    assert!(
        result.is_err(),
        "an item's reviewer error must surface as run_canary's own error"
    );

    let attempted = driver.attempted.into_inner().unwrap();
    assert!(
        attempted.contains(&"i1.rs".to_string()),
        "i1 must still be attempted despite i2's error; got {attempted:?}"
    );
    assert!(
        attempted.contains(&"i3.rs".to_string()),
        "i3 must still be attempted despite i2's error (run-to-completion, not fail-fast); \
         got {attempted:?}"
    );
    assert!(
        attempted.contains(&"i2.rs".to_string()),
        "the erroring item itself was of course attempted; got {attempted:?}"
    );

    // The aggregation loop walks scored outcomes in fixed corpus order and returns the
    // FIRST error by that order: i1 (before i2) is already durable, i3 (after i2, though
    // fully computed) never reaches the store.
    let events = store
        .read_stream(STREAM, 0, Direction::Forward)
        .expect("the canary stream reads back");
    let decoded: Vec<CanaryOutcome> = events
        .iter()
        .filter_map(CanaryOutcome::from_event)
        .collect();
    assert_eq!(
        decoded.len(),
        1,
        "only the item before the first error (i1) is durable; got {decoded:?}"
    );
    assert_eq!(decoded[0].id, "i1");
}
