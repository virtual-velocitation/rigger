//! Operator metrics, projected from the conductor's event log.
//!
//! This is a third CQRS read-model over the one append-only event stream, beside
//! [`crate::ledger`] (durable run state) and [`crate::contextgraph`] (the
//! knowledge graph). It answers a distinct question - *is the implement -> review
//! loop earning its cost?* - by folding an ordered `&[Event]` slice into a
//! [`Metrics`] summary: first-pass yield, per-gate remediation counts, escalation
//! rate, and review approve/reject counts.
//!
//! Like the other projections it is a pure replay: [`project`] mirrors
//! [`crate::ledger::project`], applies each event in order, and ignores unknown or
//! malformed events so the same shared log can feed all three read-models. It
//! single-sources the event-type *vocabulary* (it imports [`ledger::TYPE_*`] and
//! [`contextgraph::TYPE_GATE_VERDICT`] rather than re-declaring them); the wire
//! *payload* structs in those modules are private, so the few fields this fold
//! needs are decoded locally with [`field_str`] / [`gate_verdict`].
//!
//! # Review approve/reject classification
//!
//! A unit's review outcome is read off the conductor's two review paths:
//!
//! - **Per-unit review** (`run_single_stage`): the conductor emits a `verified`
//!   `UnitStatus` once the gates pass, *then* runs the adjudicator. An approve
//!   integrates; a reject loops back into `UnitFailed` with no intervening
//!   `reviewed`. So a per-unit reject is detected by `verified`-then-`UnitFailed`,
//!   and a gate failure (which never emits `verified`) is correctly not counted.
//! - **Fan-out / standalone review** (`run_fan_out_review_loop`): this path emits
//!   `reviewed` on approve but only a bare `UnitFailed` on reject, with **no
//!   `verified`**. There is no per-unit cause discriminator on `UnitFailed`, so a
//!   fan-out reject is inferred from the unit's `UnitStarted` carrying an **empty
//!   `agent`** (the necessary condition for `is_fan_out`).
//!
//! The empty-`agent` signal is a **lossy heuristic, not an exact classifier**: an
//! empty agent is *necessary* for a fan-out / review-only stage but not
//! *sufficient*. A non-fan-out stage may also be authored with an empty `agent`
//! (config validation never requires one) and run real code gates; its bare
//! gate-failure `UnitFailed` then has the same shape as a review reject and is
//! counted as one. This **known, accepted false positive** is the price of the
//! conductor not stamping a cause on `UnitFailed`; an exact split would need a
//! `cause` field on the producer, which is out of this read-model's scope. The
//! regression test `agentless_gate_failure_is_counted_as_a_review_reject_known_false_positive`
//! pins the trade-off. The approve count (`reviewed`) is exact.
//!
//! Reject counts are reported in **aggregate only**. The spec allows splitting
//! rejects by flagging tier *where the actor metadata makes it derivable*, but the
//! conductor stamps no `META_ACTOR` on either `UnitFailed` emit site (both route
//! through `emit_with_actor("")`), so a per-tier split would be permanently empty
//! on any real log - the spec's "otherwise report the aggregate" applies.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::contextgraph::TYPE_GATE_VERDICT;
use crate::eventstore::Event;
use crate::ledger::{
    TYPE_UNIT_ESCALATED, TYPE_UNIT_FAILED, TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED,
    TYPE_UNIT_STATUS,
};

/// Pass/fail tallies for one gate id across a run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateCounts {
    /// `GateVerdict` events with `pass:true` (artifact-tagged ones excluded).
    pub pass: u64,
    /// `GateVerdict` events with `pass:false` - the remediation signal.
    pub fail: u64,
}

impl GateCounts {
    /// Total real gate runs recorded for this gate.
    pub fn total(&self) -> u64 {
        self.pass + self.fail
    }
}

/// The operator-facing metrics for one run, folded from the event log.
///
/// Percentages (`first_pass_yield`, `escalation_rate`) are derived on demand
/// rather than stored, so the struct stays a plain count aggregate and can derive
/// [`Eq`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    /// Distinct units that emitted a `UnitStarted` (deduplicated across resumes).
    pub units_started: u64,
    /// Units that reached `Integrated` with zero `UnitFailed` for their id - a
    /// clean first pass. The numerator of first-pass yield.
    pub first_pass_clean: u64,
    /// Per-gate pass/fail tallies, excluding the artifact-tagged integrate-time
    /// `GateVerdict`s. Sorted by gate id (`BTreeMap`) for stable reporting.
    pub gates: BTreeMap<String, GateCounts>,
    /// Distinct units that emitted `UnitEscalated`. The numerator of the
    /// escalation rate.
    pub units_escalated: u64,
    /// Reviews that approved: a `reviewed` `UnitStatus` (exact count).
    pub review_approve: u64,
    /// Reviews that rejected and looped back into `UnitFailed` without
    /// integrating on that attempt. Aggregate only.
    ///
    /// Detected on both conductor review paths: the per-unit path arms on a
    /// `verified` `UnitStatus` then counts a `UnitFailed`-while-armed; the
    /// fan-out / review-only path is inferred from an empty-`agent` `UnitStarted`
    /// then a `UnitFailed`. The fan-out signal is **lossy** - see the module doc
    /// for the known/accepted false positive (an agentless gated stage's bare gate
    /// failure).
    pub review_reject: u64,
}

impl Metrics {
    /// First-pass yield as a fraction in `[0.0, 1.0]`: clean first passes over
    /// total units started. Zero when no units started (no division by zero).
    pub fn first_pass_yield(&self) -> f64 {
        ratio(self.first_pass_clean, self.units_started)
    }

    /// Escalation rate as a fraction in `[0.0, 1.0]`: escalated units over total
    /// units started. Zero when no units started.
    pub fn escalation_rate(&self) -> f64 {
        ratio(self.units_escalated, self.units_started)
    }
}

/// A guarded fraction: `num / den`, or `0.0` when `den == 0` (never `NaN`).
fn ratio(num: u64, den: u64) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

/// Per-unit fold state, keyed by unit id so concurrently-interleaved units in the
/// append-only stream never bleed into each other.
#[derive(Default)]
struct UnitFold {
    /// Saw a real `UnitStarted` (the dedup + numerator-subset guard).
    started: bool,
    /// The unit's `UnitStarted` carried an empty `agent` - the necessary (lossy)
    /// signal for a fan-out / review-only stage whose reject is a bare
    /// `UnitFailed`.
    review_only: bool,
    /// A `verified` `UnitStatus` is in flight (per-unit review armed): the next
    /// `UnitFailed` for this id is a review reject, not a gate failure.
    armed: bool,
    /// This unit emitted at least one `UnitFailed`, so it cannot be a clean first
    /// pass even if it later integrates.
    failed: bool,
    /// This unit reached `Integrated`.
    integrated: bool,
    /// This unit's escalation has already been tallied (idempotency guard against
    /// a malformed double-emit; the conductor emits `UnitEscalated` once per id).
    escalated: bool,
}

/// Fold an ordered event slice into the operator [`Metrics`], mirroring
/// [`crate::ledger::project`]. Pure and replay-safe: unknown event types and
/// malformed payloads are ignored, so the same shared log feeds this read-model
/// alongside the ledger and the context graph.
pub fn project(events: &[Event]) -> Metrics {
    let mut units: BTreeMap<String, UnitFold> = BTreeMap::new();
    let mut metrics = Metrics::default();

    for e in events {
        match e.type_.as_str() {
            TYPE_UNIT_STARTED => {
                // The conductor emits UnitStarted with `id` and `agent`; on resume
                // it re-emits for a not-yet-integrated unit, so dedup on first sight
                // keeps units_started (the yield / escalation denominator) honest.
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                let u = units.entry(id).or_default();
                if !u.started {
                    u.started = true;
                    // Empty agent is the necessary (lossy) marker of a fan-out /
                    // review-only stage; see the module doc for the false positive.
                    u.review_only = field_str(e, "agent").unwrap_or_default().is_empty();
                    metrics.units_started += 1;
                }
            }
            TYPE_UNIT_STATUS => {
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                let Some(status) = field_str(e, "status") else {
                    continue;
                };
                let u = units.entry(id).or_default();
                match status.as_str() {
                    // Per-unit review: `verified` means gates passed and the
                    // adjudicator is about to run; arm the reject detector so a
                    // following UnitFailed is read as a review reject (a gate
                    // failure never reaches `verified`).
                    "verified" => u.armed = true,
                    // A `reviewed` status is the standalone/fan-out approve signal,
                    // exact. Gate the increment on a started unit so the count stays
                    // a subset of units_started.
                    "reviewed" if u.started => metrics.review_approve += 1,
                    _ => {}
                }
            }
            TYPE_UNIT_FAILED => {
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                let u = units.entry(id).or_default();
                u.failed = true;
                // Classify the failure as a review reject on either review path,
                // gated on a started unit so review_reject stays a subset of
                // units_started:
                //   - per-unit: a `verified` status armed the reject;
                //   - fan-out / review-only: the lossy empty-`agent` heuristic (see
                //     the module doc for the accepted false positive).
                // A gate / spawn failure of a normal, agent-named unit matches
                // neither arm and is correctly not counted.
                if u.started && (u.armed || u.review_only) {
                    metrics.review_reject += 1;
                }
                // The reject (or remediation) is consumed; disarm so a later retry's
                // gate failure on the same id is not double-counted.
                u.armed = false;
            }
            TYPE_UNIT_ESCALATED => {
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                let u = units.entry(id).or_default();
                if !u.escalated {
                    u.escalated = true;
                    metrics.units_escalated += 1;
                }
            }
            TYPE_UNIT_INTEGRATED => {
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                units.entry(id).or_default().integrated = true;
            }
            TYPE_GATE_VERDICT => {
                // Decode only the three fields this read-model needs. Artifact-tagged
                // verdicts are the integrate-time GATED_BY bookkeeping (one per
                // changed file), NOT real gate runs, so exclude them - the count
                // must reflect gate noise, not how many files a unit touched.
                let Some(v) = gate_verdict(e) else {
                    continue;
                };
                if !v.artifact.is_empty() {
                    continue;
                }
                let counts = metrics.gates.entry(v.gate).or_default();
                if v.pass {
                    counts.pass += 1;
                } else {
                    counts.fail += 1;
                }
            }
            // Unknown / foreign event types (DecisionMade, LessonLearned, ...) are
            // ignored so the same shared log feeds every read-model.
            _ => {}
        }
    }

    // First-pass-clean numerator: integrated with zero failures, gated on a real
    // UnitStarted so the numerator stays a subset of units_started.
    metrics.first_pass_clean = units
        .values()
        .filter(|u| u.started && u.integrated && !u.failed)
        .count() as u64;

    metrics
}

/// The minimal `GateVerdict` view this fold needs. The canonical payload struct in
/// [`crate::contextgraph`] is private, so the three load-bearing fields are decoded
/// locally; the event-*type* constant is still single-sourced from there.
#[derive(Deserialize)]
struct GateVerdictView {
    gate: String,
    #[serde(default)]
    pass: bool,
    #[serde(default)]
    artifact: String,
}

/// Decode a `GateVerdict` payload, or `None` for a malformed one (the sentinel arm
/// that keeps the fold panic-free on a foreign / partial event).
fn gate_verdict(e: &Event) -> Option<GateVerdictView> {
    serde_json::from_slice(&e.data).ok()
}

/// Pull a single string field out of an event's JSON payload, or `None` if the
/// payload is not a JSON object or the field is absent / not a string. This is the
/// sentinel that lets the fold ignore malformed events without panicking.
fn field_str(e: &Event, key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(&e.data).ok()?;
    value.get(key)?.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(type_: &str, data: &str) -> Event {
        Event::new(type_, data.as_bytes().to_vec())
    }

    fn started(id: &str, agent: &str) -> Event {
        ev(
            TYPE_UNIT_STARTED,
            &format!(r#"{{"id":"{id}","agent":"{agent}"}}"#),
        )
    }

    fn status(id: &str, status: &str) -> Event {
        ev(
            TYPE_UNIT_STATUS,
            &format!(r#"{{"id":"{id}","status":"{status}"}}"#),
        )
    }

    fn failed(id: &str) -> Event {
        ev(
            TYPE_UNIT_FAILED,
            &format!(r#"{{"id":"{id}","attempts":1}}"#),
        )
    }

    fn integrated(id: &str) -> Event {
        ev(
            TYPE_UNIT_INTEGRATED,
            &format!(r#"{{"id":"{id}","commit":"abc"}}"#),
        )
    }

    fn escalated(id: &str) -> Event {
        ev(TYPE_UNIT_ESCALATED, &format!(r#"{{"id":"{id}"}}"#))
    }

    fn verdict(gate: &str, pass: bool) -> Event {
        ev(
            TYPE_GATE_VERDICT,
            &format!(r#"{{"gate":"{gate}","pass":{pass}}}"#),
        )
    }

    fn artifact_verdict(gate: &str, artifact: &str) -> Event {
        ev(
            TYPE_GATE_VERDICT,
            &format!(r#"{{"gate":"{gate}","pass":true,"artifact":"{artifact}"}}"#),
        )
    }

    #[test]
    fn empty_log_has_zeroed_metrics_and_no_nan() {
        let m = project(&[]);
        assert_eq!(m, Metrics::default());
        assert_eq!(m.first_pass_yield(), 0.0);
        assert_eq!(m.escalation_rate(), 0.0);
    }

    #[test]
    fn projects_every_metric_from_a_synthetic_run() {
        // Two implement units: `a` is a clean first pass; `b` fails once then
        // integrates (not a clean first pass). One per-unit review approves (`a`),
        // one gate runs twice (one fail then one pass = remediation noise), one
        // escalated unit `c`, plus an artifact-tagged verdict that must be ignored.
        let events = vec![
            started("a", "impl"),
            verdict("build", true),
            status("a", "verified"),
            status("a", "reviewed"),
            integrated("a"),
            artifact_verdict("build", "src/a.rs"), // GATED_BY bookkeeping - excluded
            started("b", "impl"),
            verdict("build", false),
            failed("b"),
            verdict("build", true),
            integrated("b"),
            started("c", "impl"),
            failed("c"),
            escalated("c"),
        ];
        let m = project(&events);

        assert_eq!(m.units_started, 3);
        assert_eq!(m.first_pass_clean, 1); // only `a`
        assert_eq!(m.first_pass_yield(), 1.0 / 3.0);

        let build = m.gates.get("build").expect("build gate tallied");
        // 3 real runs (a:pass, b:fail, b:pass) + the artifact verdict EXCLUDED.
        assert_eq!(build.pass, 2);
        assert_eq!(build.fail, 1);
        assert_eq!(build.total(), 3);

        assert_eq!(m.units_escalated, 1);
        assert_eq!(m.escalation_rate(), 1.0 / 3.0);

        assert_eq!(m.review_approve, 1); // `a` reviewed
        assert_eq!(m.review_reject, 0);
    }

    #[test]
    fn folds_a_synthetic_slice_into_a_fully_asserted_metrics_value() {
        // Spec/01 line 34: the projection is covered by a unit test that folds a
        // synthetic event slice and asserts EACH metric value. Where
        // `projects_every_metric_from_a_synthetic_run` checks fields piecemeal, this
        // test exercises ALL metrics at once - both review outcomes (approve AND
        // reject), two gates each with pass+fail, a clean first pass and a failed
        // one, and an escalation - then asserts the WHOLE `Metrics` value via a
        // single `Eq` against a fully-constructed expected struct. A field added to
        // `Metrics` later cannot silently escape this assertion the way it could a
        // set of per-field asserts.
        let events = vec![
            // `clean`: per-unit review approve, zero failures => first-pass clean.
            started("clean", "impl"),
            verdict("build", true),
            verdict("clippy", true),
            status("clean", "verified"),
            status("clean", "reviewed"),
            integrated("clean"),
            artifact_verdict("build", "src/clean.rs"), // GATED_BY bookkeeping - excluded
            // `reject`: per-unit review reject (verified then UnitFailed), retries,
            // then integrates - failed once so NOT a clean first pass.
            started("reject", "impl"),
            verdict("build", true),
            verdict("clippy", false),
            status("reject", "verified"),
            failed("reject"), // review reject (armed by `verified`)
            verdict("clippy", true),
            status("reject", "verified"),
            status("reject", "reviewed"),
            integrated("reject"),
            // `esc`: a named unit that fails and escalates - no review activity, so
            // its UnitFailed is a remediation, not a review reject.
            started("esc", "impl"),
            verdict("build", false),
            failed("esc"),
            escalated("esc"),
        ];

        let mut gates = BTreeMap::new();
        // build: clean(pass) + reject(pass) + esc(fail) = 2 pass / 1 fail. The
        // artifact-tagged build verdict is excluded.
        gates.insert("build".to_string(), GateCounts { pass: 2, fail: 1 });
        // clippy: clean(pass) + reject(fail) + reject(pass) = 2 pass / 1 fail.
        gates.insert("clippy".to_string(), GateCounts { pass: 2, fail: 1 });
        let expected = Metrics {
            units_started: 3,
            first_pass_clean: 1, // only `clean`
            gates,
            units_escalated: 1, // `esc`
            review_approve: 2,  // `clean` + `reject` (the retry approved)
            review_reject: 1,   // `reject`'s verified-then-UnitFailed
        };

        let m = project(&events);
        // The whole value in one assertion - every metric, exactly.
        assert_eq!(m, expected);
        // Derived ratios over the same fold (never NaN; guarded denominators).
        assert_eq!(m.first_pass_yield(), 1.0 / 3.0);
        assert_eq!(m.escalation_rate(), 1.0 / 3.0);
        assert_eq!(m.gates["build"].total(), 3);
        assert_eq!(m.gates["clippy"].total(), 3);
    }

    #[test]
    fn counts_review_rejects_on_both_per_unit_and_fan_out_paths() {
        // Per-unit reject: `verified` arms, then UnitFailed-while-armed = reject.
        // Fan-out reject: empty-agent UnitStarted, then a bare UnitFailed = reject.
        let events = vec![
            started("p", "impl"),
            status("p", "verified"),
            failed("p"), // per-unit review reject
            started("f", ""),
            failed("f"), // fan-out / review-only reject
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 2);
        assert_eq!(m.review_approve, 0);
    }

    #[test]
    fn fan_out_reject_then_approve_counts_one_each() {
        // A fan-out review stage that rejects once (UnitFailed) and on the next
        // attempt approves (reviewed) yields exactly one reject and one approve.
        let events = vec![
            started("r", ""),
            failed("r"),             // reject
            status("r", "reviewed"), // approve on the retry
            integrated("r"),
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 1);
        assert_eq!(m.review_approve, 1);
    }

    #[test]
    fn a_gate_failure_of_a_named_unit_is_not_a_review_reject() {
        // A normal agent-named unit failing its gates (no `verified`, no `reviewed`)
        // is a remediation, NOT a review reject.
        let events = vec![
            started("g", "impl"),
            verdict("build", false),
            failed("g"),
            verdict("build", true),
            integrated("g"),
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 0);
        assert_eq!(m.review_approve, 0);
        assert_eq!(m.first_pass_clean, 0); // it failed once
    }

    #[test]
    fn a_spawn_crash_of_a_named_unit_is_not_a_review_reject() {
        // A unit that crashes before any gate/review (UnitFailed with no prior
        // verdict/verified) on a named agent is not a review reject.
        let events = vec![started("x", "impl"), failed("x")];
        let m = project(&events);
        assert_eq!(m.review_reject, 0);
    }

    #[test]
    fn agentless_gate_failure_is_counted_as_a_review_reject_known_false_positive() {
        // Pins the accepted LOSSY false positive (module doc): a non-fan-out stage
        // authored with an empty `agent` that runs real gates and fails emits a bare
        // UnitFailed (no verified, no reviewed) indistinguishable from a fan-out
        // reject, so it IS counted as a review reject. Exact classification would
        // need a `cause` field on UnitFailed, out of this read-model's scope.
        let events = vec![
            started("checkpoint", ""), // empty agent => review_only heuristic
            verdict("build", false),
            failed("checkpoint"),
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 1);
        assert_eq!(m.review_approve, 0);
        // The failing gate is still tallied as a real gate run.
        assert_eq!(m.gates.get("build").unwrap().fail, 1);
    }

    #[test]
    fn duplicate_unit_started_counts_the_unit_once() {
        // On resume the conductor re-emits UnitStarted for a not-yet-integrated
        // unit; the dedup guard keeps units_started (the yield/escalation
        // denominator) at 1.
        let events = vec![
            started("u", "impl"),
            failed("u"),
            started("u", "impl"), // resume re-emit
            integrated("u"),
        ];
        let m = project(&events);
        assert_eq!(m.units_started, 1);
        assert_eq!(m.first_pass_clean, 0); // failed before integrating
    }

    #[test]
    fn interleaved_units_keep_per_id_review_state() {
        // Two units' events interleave in the append-only stream (run_batch spawns
        // under a thread scope). Per-id state must not bleed: `a` approves, `b`
        // rejects.
        let events = vec![
            started("a", "impl"),
            started("b", "impl"),
            status("a", "verified"),
            status("b", "verified"),
            status("a", "reviewed"),
            integrated("a"),
            failed("b"), // b's verified arm => review reject for b
        ];
        let m = project(&events);
        assert_eq!(m.review_approve, 1);
        assert_eq!(m.review_reject, 1);
        assert_eq!(m.first_pass_clean, 1); // only `a`
    }

    #[test]
    fn escalation_is_counted_once_per_unit() {
        let events = vec![
            started("a", "impl"),
            started("b", "impl"),
            failed("a"),
            escalated("a"),
        ];
        let m = project(&events);
        assert_eq!(m.units_escalated, 1);
        assert_eq!(m.units_started, 2);
        assert_eq!(m.escalation_rate(), 0.5);
    }

    #[test]
    fn artifact_tagged_verdicts_are_excluded_from_gate_counts() {
        // Only the real (artifact-free) gate runs count; the per-file GATED_BY
        // verdicts emitted at integrate time are bookkeeping, not gate noise.
        let events = vec![
            verdict("clippy", true),
            artifact_verdict("clippy", "src/a.rs"),
            artifact_verdict("clippy", "src/b.rs"),
            verdict("clippy", false),
        ];
        let m = project(&events);
        let clippy = m.gates.get("clippy").unwrap();
        assert_eq!(clippy.pass, 1);
        assert_eq!(clippy.fail, 1);
        assert_eq!(clippy.total(), 2);
    }

    #[test]
    fn foreign_and_malformed_events_are_ignored_without_panicking() {
        // The shared log carries foreign types (DecisionMade) and, defensively,
        // malformed payloads. The fold must ignore them all and leave the metrics
        // for the valid run untouched - exercising the .ok()? / get()? sentinels.
        let events = vec![
            ev("DecisionMade", r#"{"id":"d","summary":"x"}"#), // foreign type
            ev(TYPE_UNIT_STARTED, "not json at all"),          // malformed payload
            ev(TYPE_UNIT_STARTED, r#"{"agent":"impl"}"#),      // id-less UnitStarted
            ev(TYPE_GATE_VERDICT, r#"{"pass":true}"#),         // gate-less verdict
            ev(TYPE_UNIT_STATUS, r#"{"id":"a"}"#),             // status-less
            started("a", "impl"),                              // the only valid unit
            verdict("build", true),
            status("a", "verified"),
            status("a", "reviewed"),
            integrated("a"),
        ];
        let m = project(&events);
        assert_eq!(m.units_started, 1);
        assert_eq!(m.first_pass_clean, 1);
        assert_eq!(m.review_approve, 1);
        assert_eq!(m.review_reject, 0);
        assert_eq!(m.gates.get("build").unwrap().pass, 1);
        // The gate-less malformed verdict must not have created an entry.
        assert_eq!(m.gates.len(), 1);
    }
}
