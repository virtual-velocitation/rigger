//! Canary-corpus fixtures.

use rigger::canary_store::CanaryItem;
use rigger::conductor::{AgentResult, Error};
use rigger::contextgraph::TYPE_REVIEW_FINDING;

/// The finding summary that marks a "critical" finding - the substring a scripted adjudicator
/// looks for in its own prompt (which embeds every finding's summary), mirroring how the live
/// adjudicator prompt actually carries findings.
pub const CRITICAL_SUMMARY: &str = "CRIT defect here";

/// A canary item `id` of `defect_class`, anchored at `<id>.rs` with a one-line review body.
pub fn item(id: &str, defect_class: &str, planted: bool, verdict: &str, tier: &str) -> CanaryItem {
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

/// A canary item `id` anchored at `<id>.rs`: a planted one carries the `off-by-one` defect class,
/// a control carries `none`.
pub fn planted_item(id: &str, planted: bool, verdict: &str, tier: &str) -> CanaryItem {
    let defect_class = if planted { "off-by-one" } else { "none" };
    item(id, defect_class, planted, verdict, tier)
}

/// A [`planted_item`] anchored at `anchor`.
pub fn anchored_item(
    id: &str,
    anchor: &str,
    planted: bool,
    verdict: &str,
    tier: &str,
) -> CanaryItem {
    CanaryItem {
        anchor: anchor.into(),
        ..planted_item(id, planted, verdict, tier)
    }
}

/// An [`anchored_item`] with no expected tier whose review body carries the comment `marker`.
pub fn marked_item(
    id: &str,
    anchor: &str,
    planted: bool,
    verdict: &str,
    marker: &str,
) -> CanaryItem {
    CanaryItem {
        review: format!("fn {id}() {{ /* {marker} */ }}"),
        ..anchored_item(id, anchor, planted, verdict, "")
    }
}

/// A scripted adjudicator's result: reject iff its prompt carries a [`CRITICAL_SUMMARY`]
/// finding, approve otherwise.
pub fn critical_verdict(prompt: &str) -> AgentResult {
    let verdict = if prompt.contains(CRITICAL_SUMMARY) {
        "reject"
    } else {
        "approve"
    };
    AgentResult {
        output: format!("{{\"verdict\":\"{verdict}\"}}"),
        resolved_model: String::new(),
        ..Default::default()
    }
}

/// A scripted reviewer's turn: emit `reviewer`'s one finding - a [`CRITICAL_SUMMARY`] about
/// `anchor` when it `catches`, otherwise a minor nit about an unrelated file - and return the
/// reviewed result.
pub fn emit_review_finding(
    emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    reviewer: &str,
    anchor: &str,
    catches: bool,
) -> Result<AgentResult, Error> {
    let finding = if catches {
        serde_json::json!({"id": format!("f-{reviewer}"), "by": reviewer, "summary": CRITICAL_SUMMARY, "about": [anchor]})
    } else {
        serde_json::json!({"id": format!("f-{reviewer}"), "by": reviewer, "summary": "minor style nit", "about": ["other.rs"]})
    };
    emit(TYPE_REVIEW_FINDING, finding)?;
    Ok(AgentResult {
        output: "reviewed".into(),
        resolved_model: String::new(),
        ..Default::default()
    })
}

/// The anchor a canary review prompt names: the review header's first code span.
pub fn anchor_of(prompt: &str) -> String {
    rigger::spec::code_spans(prompt)
        .into_iter()
        .next()
        .unwrap_or_default()
}
