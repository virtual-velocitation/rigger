//! Spec 67, criterion 4 (REVIEW TIERS NAME THEIR TARGETS) - the driver renders the conductor's
//! stamped review roster inside the action phrase.
//!
//! Like `tests/worker_persona_label_periphery.rs` does for criterion 2's `workerLabel`, this file
//! extracts the real declarations VERBATIM from the shipped `workflows/rigger.js` - never
//! hand-copied, so a future edit is exercised here without a separate update - and runs them for
//! real under `node`. `workerLabel` (and the `ROSTER_VERB` table it now reads alongside
//! `PERSONA_VERB`) depends on no injected global, so nothing needs stubbing.

mod common;

use common::repo::worker_label;

/// Run the REAL `workerLabel(req)` - extracted verbatim along with `PERSONA_VERB`,
/// `ROSTER_VERB`, and the `personaOf`/`firstSentence`/`roleAttempt` helpers it calls - under a
/// real `node` subprocess, with `req.reviews` set to `reviews` (pass `None` to omit the field
/// entirely - the "an older conductor" case). Returns the rendered label, or `None` when `node`
/// is not on PATH - the same graceful-absence contract this crate's other JS-extraction tests
/// establish.
fn run_worker_label_with_reviews(
    id: &str,
    title: &str,
    reviews: Option<&[&str]>,
) -> Option<String> {
    worker_label(id, title, None, reviews)
}

/// The worker label rendered for spawn `id` titled `title` with review roster `reviews` is
/// exactly `expected`; a graceful no-op when `node` is unavailable.
fn assert_rendered_label(id: &str, title: &str, reviews: Option<&[&str]>, expected: &str) {
    let Some(label) = run_worker_label_with_reviews(id, title, reviews) else {
        return; // node unavailable; graceful absence.
    };
    assert_eq!(label, expected);
}

rigger::test_cases! {
    /// The adversary's action phrase inlines the routed roster the conductor stamped: "challenge
    /// the findings, assumptions, and rigor of <roster>" - the roster is the conductor's stamped
    /// `lens:<id>` tokens, joined verbatim, never re-derived or re-cased by the driver.
    the_adversarys_roster_renders_inside_its_action_phrase: assert_rendered_label(
        "u1/adversary#0",
        "challenge the lenses' findings.",
        Some(&["lens:sdet", "lens:architecture-reviewer"]),
        "Adversary - challenge the findings, assumptions, and rigor of lens:sdet, \
         lens:architecture-reviewer #0: challenge the lenses' findings.",
    );
}

rigger::test_cases! {
    /// The adjudicator's action phrase inlines lenses PLUS the adversary: "weigh <roster> and
    /// rule" - proving the roster is inserted INSIDE the phrase (between "weigh" and "and rule"),
    /// not merely appended after it.
    the_adjudicators_roster_renders_inside_its_action_phrase: assert_rendered_label(
        "u1/adjudicator#1",
        "weigh the lenses and the adversary.",
        Some(&["lens:sdet", "adversary"]),
        "Adjudicator - weigh lens:sdet, adversary and rule #1: weigh the lenses and the \
         adversary.",
    );
}

/// A roster-less item (an older conductor, or a panel with no lenses/adversary) renders the
/// base phrase without ANY roster clause - `req.reviews` entirely ABSENT from the wave item,
/// exactly as an older conductor's wire shape would arrive.
#[test]
fn a_roster_less_item_renders_the_phrase_without_a_roster_clause() {
    let Some(adversary) =
        run_worker_label_with_reviews("u2/adversary#0", "challenge the findings.", None)
    else {
        return;
    };
    assert_eq!(
        adversary,
        "Adversary - challenge the findings, assumptions, and rigor #0: challenge the findings."
    );

    let adjudicator = run_worker_label_with_reviews("u2/adjudicator#0", "weigh and rule.", None)
        .expect("node already proven available above");
    assert_eq!(
        adjudicator,
        "Adjudicator - weigh and rule #0: weigh and rule."
    );
}

/// An EMPTY `req.reviews` array (a panel the conductor routed with no lenses AND no adversary -
/// e.g. an empty panel that still names an adjudicator) renders identically to an absent field:
/// the base phrase, no roster clause - proven distinctly from the absent-field case above so a
/// future `reviews: []` regression on the conductor side is caught the same way.
#[test]
fn an_empty_reviews_array_renders_identically_to_an_absent_one() {
    let Some(label) =
        run_worker_label_with_reviews("u3/adjudicator#0", "weigh and rule.", Some(&[]))
    else {
        return;
    };
    assert_eq!(label, "Adjudicator - weigh and rule #0: weigh and rule.");
}

rigger::test_cases! {
    /// A roster on a role with NO `ROSTER_VERB` entry (a lens, or any non-review-tier role) is
    /// NEVER rendered - "never a fabricated or stale roster" cuts both ways: the driver must not
    /// invent a roster clause for a role the conductor never intends one for, even if `req.reviews`
    /// happened to carry a (malformed) value.
    a_roster_on_a_role_with_no_roster_verb_entry_is_never_rendered: assert_rendered_label(
        "u4/lens:sdet#0",
        "evaluate whether the tests are discriminating.",
        Some(&["lens:sdet"]),
        "Lens:SDET - evaluate testing effectiveness #0: evaluate whether the tests are \
         discriminating.",
    );
}

rigger::test_cases! {
    /// A single-entry roster renders with no stray separator - proving the join is exercised at
    /// both cardinalities, not merely assumed correct from the two-entry cases above.
    a_single_entry_roster_renders_with_no_stray_separator: assert_rendered_label(
        "u5/adversary#2",
        "challenge the one lens.",
        Some(&["lens:sdet"]),
        "Adversary - challenge the findings, assumptions, and rigor of lens:sdet #2: \
         challenge the one lens.",
    );
}

/// adv-u67c4-plancritique-persona-roster-composition-untested: criterion 2's persona override
/// (`req.unit === "plan-critique"` forces the `Plan-Critique` persona regardless of the role
/// half) COMPOSED with criterion 4's roster injection (a non-empty `req.reviews` inlined via
/// `ROSTER_VERB[role]`) - the exact shape the plan-critique gate's real adjudicator spawn
/// produces, since `plan_critique_loop` always stamps `adjudicator_roster(&[], adversary)` (the
/// bare adversary token - the DAG-level critique names no lens tier). Every other test in this
/// file exercises the roster with NO `req.unit`, and `worker_persona_label_periphery.rs` exercises
/// `req.unit` with NO roster - `workerLabel` derives both from the SAME `req` object in one
/// function body, so this combination is a distinct seam neither file's existing coverage pins.
/// Correct today; this proves a future edit to `PERSONA_VERB`/`ROSTER_VERB`/the `req.unit` branch
/// cannot silently regress it while every roster-only or persona-only test above keeps passing.
#[test]
fn the_plan_critiques_adjudicator_composes_its_persona_override_with_its_roster() {
    let Some(label) = worker_label(
        "plan-critique/adjudicator#0",
        "review the proposed unit DAG.",
        Some("plan-critique"),
        Some(&["adversary"]),
    ) else {
        return; // node unavailable; graceful absence.
    };
    assert_eq!(
        label,
        "Plan-Critique - weigh adversary and rule #0: review the proposed unit DAG."
    );
}
