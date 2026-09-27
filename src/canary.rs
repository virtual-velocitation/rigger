//! The canary's pure wire model (spec 93, criterion 1): the scored-outcome and
//! run-header types the canary stream carries, and the [`latest_run`] fold that scopes a
//! read to the last batch. This is the half [`crate::metrics::project_canary`] (a declared
//! `core` module) actually needs - the corpus loader, the review-panel runner and every
//! other piece that spawns agents or touches the event store lives in
//! [`crate::canary_store`], the write half. See that module's own doc for the split
//! rationale (mirrors [`crate::spawn`]/[`crate::spawn_store`]).
//!
//! `rigger canary` and its runner ([`crate::canary_store::run_canary`]) are unaffected -
//! this split moves definitions, not behavior. `crate::canary_store` re-exports nothing
//! back; every existing `canary::X` call site for a moved item was renamed to
//! `canary_store::X` at the same time as this split landed.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::eventstore::Event;
use crate::ledger::TYPE_UNIT_STATUS;

/// The event stream the canary run's scored outcomes land on - the canary NAMESPACE. It
/// is DISTINCT from [`conductor::STREAM`](crate::conductor::STREAM) (the run stream the
/// operator metrics fold reads), so a canary run never perturbs a project's first-pass
/// yield / review counts and `rigger stats --canary` reads only these scored outcomes.
pub const STREAM: &str = "canary";

/// The `UnitStatus` status token a per-item canary outcome rides (spec 13 forbids new
/// event types). A canary outcome is a `UnitStatus` on the canary stream, so it never
/// folds into run state - `ledger::Status::parse` returns `None` for it and the
/// run-metrics projector ignores it - exactly like the review-tier / speculation markers.
pub const STATUS_CANARY: &str = "canary";

/// The status token that OPENS one canary run (batch) on the stream. `rigger canary`
/// appends it before the batch's per-item outcomes, so `stats --canary` can scope its
/// report to the LATEST canary run (the events from the last marker onward) rather than
/// aggregating every historical run - mirroring how [`runscope::current_run`] scopes the
/// run stream by its opening `RunStarted`.
pub const STATUS_CANARY_RUN: &str = "canary-run";

/// The status token the run-level HEADER event rides (MODEL PINNING criterion): binary
/// build, corpus content hash, and every tier's ACTUALLY-resolved model id, recorded once
/// per batch AFTER every item is scored (so a live-driver resolved id observed mid-run is
/// captured before it is written). Distinct from [`STATUS_CANARY_RUN`], which only OPENS
/// the batch and stays byte-for-byte unchanged by this criterion - [`latest_run`] scopes by
/// that opening marker, and this trailing event still falls inside the scoped slice because
/// it is appended AFTER it, before the next batch's marker (if any).
pub const STATUS_CANARY_HEADER: &str = "canary-header";

/// The tier-1 label (the expert lenses, collectively) catch rate is reported for.
pub const TIER_LENS: &str = "lens";
/// The tier-2 label (the adversary) catch rate is reported for.
pub const TIER_ADVERSARY: &str = "adversary";

/// The metadata key tagging a canary outcome (and its batch marker) with the canary run
/// it belongs to.
pub const META_CANARY_BATCH: &str = "canary_batch";

/// The scored outcome of running the review panel against one canary item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanaryOutcome {
    pub id: String,
    pub defect_class: String,
    pub planted: bool,
    /// Whether the adjudicator SHOULD have rejected (a planted defect).
    pub expected_reject: bool,
    /// The tier the corpus expected to catch it (informational).
    pub expected_tier: String,
    /// The tiers that ACTUALLY raised a finding about the anchor (subset of
    /// {[`TIER_LENS`], [`TIER_ADVERSARY`]}), sorted for determinism.
    pub caught_by: Vec<String>,
    /// Whether the adjudicator approved the item.
    pub verdict_approved: bool,
    /// Whether the adjudicator's verdict matched the expectation.
    pub verdict_correct: bool,
    /// Whether the adjudicator's verdict was STABLE when the findings were re-presented
    /// in a shuffled order (the position-bias probe). Trivially `true` when there are
    /// fewer than two findings to reorder.
    pub stable: bool,
    /// The count of findings each tier RAISED for this item - regardless of whether any
    /// of them caught the planted defect - keyed by tier label ([`TIER_LENS`],
    /// [`TIER_ADVERSARY`]). This is the over-flagging / findings-volume measure: a tier
    /// that raises many findings but catches nothing is scored honestly on both axes. A
    /// tier that ran (the adversary is optional per panel) but raised nothing records `0`,
    /// never an absent key - the same "seed every known tier" discipline `tier_catch`
    /// follows, so a silent key omission never reads as "not measured".
    pub findings_raised: BTreeMap<String, u64>,
}

/// A canary-stream event: a fold-neutral `UnitStatus` carrying `data`, tagged with the batch
/// id in metadata so the fold can scope to one run.
fn canary_event(data: &serde_json::Value, batch: &str) -> Event {
    Event::new(
        TYPE_UNIT_STATUS,
        serde_json::to_vec(data).unwrap_or_default(),
    )
    .with_meta(META_CANARY_BATCH, batch)
}

impl CanaryOutcome {
    /// Serialize this outcome to its canary-stream event: a fold-neutral `UnitStatus`
    /// carrying the score in its data, tagged with the batch id in metadata so the fold
    /// can scope to one run.
    #[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // only called from the *_store sibling under core-only
    pub(crate) fn to_event(&self, batch: &str) -> Event {
        let data = json!({
            "id": self.id,
            "status": STATUS_CANARY,
            "defect_class": self.defect_class,
            "planted": self.planted,
            "expected_reject": self.expected_reject,
            "expected_tier": self.expected_tier,
            "caught_by": self.caught_by,
            "verdict_approved": self.verdict_approved,
            "verdict_correct": self.verdict_correct,
            "stable": self.stable,
            "findings_raised": self.findings_raised,
        });
        canary_event(&data, batch)
    }

    /// Decode a canary outcome from a canary-stream event, or `None` if it is not a
    /// [`STATUS_CANARY`] `UnitStatus` (a batch marker, or a malformed event). This is the
    /// ONE wire-schema authority the metrics fold reads through, so the producer and the
    /// fold can never disagree on the shape.
    pub fn from_event(e: &Event) -> Option<CanaryOutcome> {
        if e.type_ != TYPE_UNIT_STATUS {
            return None;
        }
        let v: Value = serde_json::from_slice(&e.data).ok()?;
        if v.get("status").and_then(Value::as_str) != Some(STATUS_CANARY) {
            return None;
        }
        Some(CanaryOutcome {
            id: str_field(&v, "id"),
            defect_class: str_field(&v, "defect_class"),
            planted: bool_field(&v, "planted"),
            expected_reject: bool_field(&v, "expected_reject"),
            expected_tier: str_field(&v, "expected_tier"),
            caught_by: v
                .get("caught_by")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            verdict_approved: bool_field(&v, "verdict_approved"),
            verdict_correct: bool_field(&v, "verdict_correct"),
            stable: bool_field(&v, "stable"),
            findings_raised: v
                .get("findings_raised")
                .and_then(Value::as_object)
                .map(|obj| {
                    obj.iter()
                        .map(|(k, val)| (k.clone(), val.as_u64().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn bool_field(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// The run-level facts the MODEL PINNING criterion's scorecard header renders: what built
/// this binary, what corpus content was scored, and what model id each review tier actually
/// resolved to (sourced from [`crate::conductor::AgentResult::resolved_model`], never a
/// configured alias) - so a pinned A/B arm is auditable from the scorecard alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanaryHeader {
    pub binary_build: String,
    pub corpus_hash: String,
    pub resolved_models: BTreeMap<String, String>,
}

impl CanaryHeader {
    /// Serialize this header to its canary-stream event: a fold-neutral `UnitStatus` (the
    /// [`STATUS_CANARY_HEADER`] token), tagged with the batch id like every other canary
    /// event so `stats --canary`'s scoped read finds it alongside the outcomes it describes.
    #[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // only called from the *_store sibling under core-only
    pub(crate) fn to_event(&self, batch: &str) -> Event {
        let data = json!({
            "id": batch,
            "status": STATUS_CANARY_HEADER,
            "binary_build": self.binary_build,
            "corpus_hash": self.corpus_hash,
            "resolved_models": self.resolved_models,
        });
        canary_event(&data, batch)
    }

    /// Decode a header from a canary-stream event, or `None` if it is not a
    /// [`STATUS_CANARY_HEADER`] `UnitStatus` (an outcome, a batch marker, or a malformed/
    /// legacy event predating this criterion - which must decode as ABSENT, never a crash or
    /// a fabricated header).
    pub fn from_event(e: &Event) -> Option<CanaryHeader> {
        if e.type_ != TYPE_UNIT_STATUS {
            return None;
        }
        let v: Value = serde_json::from_slice(&e.data).ok()?;
        if v.get("status").and_then(Value::as_str) != Some(STATUS_CANARY_HEADER) {
            return None;
        }
        Some(CanaryHeader {
            binary_build: str_field(&v, "binary_build"),
            corpus_hash: str_field(&v, "corpus_hash"),
            resolved_models: v
                .get("resolved_models")
                .and_then(Value::as_object)
                .map(|obj| {
                    obj.iter()
                        .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

/// Scope `events` (a canary-stream read) to the LATEST canary run: the slice from the
/// last [`STATUS_CANARY_RUN`] batch marker onward, or the whole slice when none is present
/// (a legacy or marker-less store). Mirrors [`runscope::current_run`] for the run stream.
pub fn latest_run(events: &[Event]) -> &[Event] {
    match events.iter().rposition(is_batch_marker) {
        Some(i) => &events[i..],
        None => events,
    }
}

pub(crate) fn is_batch_marker(e: &Event) -> bool {
    if e.type_ != TYPE_UNIT_STATUS {
        return false;
    }
    serde_json::from_slice::<Value>(&e.data)
        .ok()
        .and_then(|v| {
            v.get("status")
                .and_then(Value::as_str)
                .map(|s| s == STATUS_CANARY_RUN)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canary_outcome_round_trips_findings_raised_through_the_wire_event() {
        let mut findings_raised = BTreeMap::new();
        findings_raised.insert(TIER_LENS.to_string(), 4);
        findings_raised.insert(TIER_ADVERSARY.to_string(), 2);
        let outcome = CanaryOutcome {
            id: "x".into(),
            defect_class: "off-by-one".into(),
            planted: true,
            expected_reject: true,
            expected_tier: "lens".into(),
            caught_by: vec![TIER_LENS.into()],
            verdict_approved: false,
            verdict_correct: true,
            stable: true,
            findings_raised,
        };
        let event = outcome.to_event("batch");
        let decoded = CanaryOutcome::from_event(&event).expect("a canary outcome decodes back");
        assert_eq!(
            decoded, outcome,
            "findings_raised must round-trip byte-for-byte through the wire event, exactly \
             like every other CanaryOutcome field"
        );
    }

    #[test]
    fn latest_run_scopes_to_the_last_batch_marker() {
        let marker = || {
            Event::new(
                TYPE_UNIT_STATUS,
                serde_json::to_vec(&json!({"id":"b","status":STATUS_CANARY_RUN})).unwrap(),
            )
        };
        let outcome = |id: &str| {
            CanaryOutcome {
                id: id.into(),
                defect_class: "off-by-one".into(),
                planted: true,
                expected_reject: true,
                expected_tier: "lens".into(),
                caught_by: vec![TIER_LENS.into()],
                verdict_approved: false,
                verdict_correct: true,
                stable: true,
                findings_raised: BTreeMap::new(),
            }
            .to_event("b")
        };
        let events = vec![marker(), outcome("old"), marker(), outcome("new")];
        let scoped = latest_run(&events);
        let ids: Vec<String> = scoped
            .iter()
            .filter_map(CanaryOutcome::from_event)
            .map(|o| o.id)
            .collect();
        assert_eq!(
            ids,
            vec!["new".to_string()],
            "only the latest run is scoped"
        );
    }

    #[test]
    fn canary_header_from_event_is_none_for_a_legacy_or_foreign_event() {
        // A batch marker is not a header.
        let marker = Event::new(
            TYPE_UNIT_STATUS,
            serde_json::to_vec(&json!({"id":"b","status":STATUS_CANARY_RUN})).unwrap(),
        );
        assert!(CanaryHeader::from_event(&marker).is_none());
        // A per-item outcome is not a header either.
        let outcome_event = CanaryOutcome {
            id: "x".into(),
            defect_class: String::new(),
            planted: false,
            expected_reject: false,
            expected_tier: String::new(),
            caught_by: Vec::new(),
            verdict_approved: true,
            verdict_correct: true,
            stable: true,
            findings_raised: BTreeMap::new(),
        }
        .to_event("b");
        assert!(CanaryHeader::from_event(&outcome_event).is_none());
    }
}
