//! Periphery (integration) test for spec 61's PROGRESS criterion, unit u61c6: `run_canary`
//! gained a new `on_item: &(dyn Fn(&CanaryOutcome, Duration) + Sync)` parameter - a real
//! public-API surface change the mechanical `git diff BASE -- '*.rs' | grep -nE
//! '^\+.*\bpub (fn|struct|enum|trait|const|type)'` probe misses, because the pre-existing
//! `pub fn run_canary(` signature line itself is unchanged context; only a new line inside
//! its multi-line argument list was added. The diff's own hunk header still names the
//! enclosing item (`@@ -312,6 +322,7 @@ pub fn run_canary(`), which is how this surface item
//! was actually found and accounted for.
//!
//! Every one of this directory's five pre-existing canary periphery tests
//! (`canary_findings_volume_periphery.rs`, `canary_item_sharding_jobs_cap_periphery.rs`,
//! `canary_lens_fanout_periphery.rs`, `canary_unattributed_rejects_periphery.rs`, and
//! `store_content_identity_periphery.rs`) needed a one-line mechanical fix to keep compiling
//! against the new parameter - `&|_, _| {}`, a no-op - and none of them assert anything
//! about what that parameter actually does. This file closes that gap.
//!
//! The implementer's own unit tests (`src/canary.rs`, `#[cfg(test)] mod tests`) already pin
//! the same behavioral contracts - exactly-once-per-item delivery, content parity with the
//! returned report, and genuine per-item streaming rather than a batched call after every
//! worker thread joins - but only at the PRIVATE seam, using `#[cfg(test)]`-private driver
//! doubles unreachable from here. This suite re-proves them from OUTSIDE the crate, over the
//! public entry (`rigger::canary::run_canary`), with drivers written fresh for this file -
//! the same "cannot reuse canary.rs's own private `Scripted`" discipline every sibling file
//! in this directory already follows. A prior review round
//! (`sdet-u61c5-progress-hook-is-bunched-not-streaming`) flagged forward that a naive
//! implementation hooking `on_item` at the aggregation loop AFTER every `item_workers`
//! thread joins - rather than inside the per-item closure `map_ordered` runs on whichever
//! worker actually finishes an item - would pass any test that only checks "the line
//! appears before the final scorecard" while never truly streaming; the second test below is
//! built to fail exactly that regression, driven through the public entry rather than
//! canary.rs's internals.

use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use rigger::canary::{run_canary, CanaryItem, CanaryOutcome, TIER_LENS};
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, ReviewPanel};
use rigger::contextgraph::TYPE_REVIEW_FINDING;
use rigger::eventstore::sqlite::Store;

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
/// file cannot see canary.rs's private helper either (the same re-derivation every sibling
/// periphery file in this directory already performs independently).
fn anchor_of(prompt: &str) -> String {
    prompt
        .split_once('`')
        .and_then(|(_, rest)| rest.split_once('`'))
        .map(|(anchor, _)| anchor.to_string())
        .unwrap_or_default()
}

/// A scripted driver written fresh for this file: `lens-a` raises a critical finding about
/// every anchor named in `catches`, everyone else raises a benign non-catching finding, and
/// the adjudicator rejects iff it was shown a critical finding.
struct Catches {
    catches: Vec<&'static str>,
}

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
        let anchor = anchor_of(prompt);
        let catches = a.id == "lens-a" && self.catches.contains(&anchor.as_str());
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

/// spec 61 PROGRESS criterion, through the public entry: `on_item` fires exactly once per
/// corpus item - never dropped, never doubled - at both a serial (`jobs=1`) and a sharded
/// (`jobs=2`) width, and the `CanaryOutcome` it is called with is the SAME value (by
/// `PartialEq`) the corresponding entry in `run_canary`'s returned report carries, never an
/// independently-derived or partially-populated copy that could silently drift from it.
/// `main.rs`'s `format_progress_line` - the only production consumer of this hook - reads
/// `id`, `verdict_correct`, and `caught_by` straight off whatever `on_item` hands it, so a
/// hook firing with stale, default, or mismatched content would render a genuinely wrong
/// per-item progress line while the final scorecard stayed correct; that drift is exactly
/// what this content-parity check exists to catch. The three-item corpus mixes a correct
/// reject, a WRONG (missed) reject, and a correct approve, so the parity check is not
/// vacuously true over identical trivial outcomes.
#[test]
fn run_canary_calls_on_item_exactly_once_per_item_with_content_matching_the_report_through_the_public_entry(
) {
    let ids = ["lens-a", "adv", "adj"];
    let c = cfg(&ids);
    let p = panel(&["lens-a"]);
    let corpus = vec![
        item("hit", true, "reject", "lens"), // lens-a catches it: correct reject
        item("miss", true, "reject", "lens"), // nobody catches it: WRONG (expected reject)
        item("clean", false, "approve", ""), // known-good control: correct approve
    ];
    let expected_ids: BTreeSet<String> = corpus.iter().map(|i| i.id.clone()).collect();

    for jobs in [1usize, 2usize] {
        let captured: Mutex<Vec<CanaryOutcome>> = Mutex::new(Vec::new());
        let driver = Catches {
            catches: vec!["hit.rs"],
        };
        let store = Store::open(":memory:").expect("an in-memory store opens");
        let report = run_canary(&store, &driver, &c, &p, &corpus, jobs, &|o, _elapsed| {
            captured.lock().unwrap().push(o.clone());
        })
        .unwrap_or_else(|e| {
            panic!("run_canary must succeed through the public entry at jobs={jobs}: {e:?}")
        });

        let captured = captured.into_inner().unwrap();
        let captured_ids: BTreeSet<String> = captured.iter().map(|o| o.id.clone()).collect();
        assert_eq!(
            captured_ids, expected_ids,
            "on_item must fire exactly once per corpus item at jobs={jobs} (no drop, no dupe)"
        );
        assert_eq!(
            captured.len(),
            corpus.len(),
            "one on_item call per item, never a repeat, at jobs={jobs}"
        );

        for outcome in &captured {
            let reported = report
                .outcomes
                .iter()
                .find(|o| o.id == outcome.id)
                .unwrap_or_else(|| panic!("report carries no outcome for {:?}", outcome.id));
            assert_eq!(
                outcome, reported,
                "on_item's outcome for {:?} must equal the report's own entry, at jobs={jobs}",
                outcome.id
            );
        }

        let hit = captured.iter().find(|o| o.id == "hit").unwrap();
        assert!(
            hit.verdict_correct && hit.caught_by == vec![TIER_LENS.to_string()],
            "the caught item must render as a correct reject caught by the lens tier, at jobs={jobs}"
        );
        let miss = captured.iter().find(|o| o.id == "miss").unwrap();
        assert!(
            !miss.verdict_correct,
            "an uncaught planted defect must score as WRONG, at jobs={jobs}"
        );
        let clean = captured.iter().find(|o| o.id == "clean").unwrap();
        assert!(
            clean.verdict_correct && clean.caught_by.is_empty(),
            "the known-good control must render as a correct approve catching nothing, at jobs={jobs}"
        );
    }
}

/// spec 61 PROGRESS criterion, through the public entry: `on_item` fires for a fast-scoring
/// item WHILE a slower sibling item is still being scored - a genuine per-item STREAMING
/// hook, not one bunched at the aggregation loop after every `item_workers` thread joins
/// (see `run_canary`'s own doc comment, and the review finding this test exists to close,
/// `sdet-u61c5-progress-hook-is-bunched-not-streaming`, which flagged forward that a naive
/// aggregation-loop hook would still pass any test that only checks "the line appears before
/// the final scorecard").
///
/// Two items, `jobs=2` (`item_workers=2` per `spawn_budget`, so both genuinely run on
/// separate threads at once): "fast" has nothing gating it and scores immediately. Every one
/// of "slow"'s own review-tier spawns blocks on a gate that only opens once `on_item` has
/// ALREADY fired for "fast" - so "slow"'s score cannot complete before that callback ran. If
/// `on_item` were instead called only after `map_ordered` joins BOTH worker threads (the
/// exact regression this test exists to catch), `on_item(fast)` could never fire before
/// "slow" needs it - both threads would still be inside `map_ordered` at that point - so the
/// bounded wait below would exhaust its timeout and fail loudly instead of silently passing.
#[test]
fn on_item_streams_a_fast_items_score_while_a_slower_sibling_is_still_scoring_through_the_public_entry(
) {
    struct GateSlowOnFasts {
        released: Arc<(Mutex<bool>, Condvar)>,
    }
    impl AgentDriver for GateSlowOnFasts {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            _opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id != "adj" && anchor_of(prompt) == "slow.rs" {
                let (lock, cvar) = &*self.released;
                let guard = lock.lock().unwrap();
                let (_guard, result) = cvar
                    .wait_timeout_while(guard, Duration::from_secs(2), |released| !*released)
                    .unwrap();
                assert!(
                    !result.timed_out(),
                    "the slow item's own spawn ran without ever observing on_item fire for \
                     the fast item first - on_item is bunched at the aggregation loop, not \
                     streamed per item as each one genuinely completes, through the public \
                     entry"
                );
            }
            if a.id == "adj" {
                return Ok(AgentResult {
                    output: "{\"verdict\":\"approve\"}".into(),
                    resolved_model: String::new(),
                });
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

    let ids = ["lens-a", "adv", "adj"];
    let c = cfg(&ids);
    let p = panel(&["lens-a"]);
    let corpus = vec![
        item("fast", false, "approve", ""),
        item("slow", false, "approve", ""),
    ];

    let released = Arc::new((Mutex::new(false), Condvar::new()));
    let driver = GateSlowOnFasts {
        released: released.clone(),
    };
    let store = Store::open(":memory:").expect("an in-memory store opens");

    let report = run_canary(&store, &driver, &c, &p, &corpus, 2, &|o, _elapsed| {
        if o.id == "fast" {
            let (lock, cvar) = &*released;
            *lock.lock().unwrap() = true;
            cvar.notify_all();
        }
    })
    .expect("run_canary succeeds through the public entry");

    assert_eq!(
        report.outcomes.len(),
        2,
        "both items still score to completion once the gate opens"
    );
}
