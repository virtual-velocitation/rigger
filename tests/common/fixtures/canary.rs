//! Canary-corpus fixtures.

use rigger::canary_store::CanaryItem;

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
