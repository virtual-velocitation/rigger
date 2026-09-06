//! Spec 67, criterion 4 (REVIEW TIERS NAME THEIR TARGETS) - the driver renders the conductor's
//! stamped review roster inside the action phrase.
//!
//! Like `tests/worker_persona_label_periphery.rs` does for criterion 2's `workerLabel`, this file
//! extracts the real declarations VERBATIM from the shipped `workflows/rigger.js` - never
//! hand-copied, so a future edit is exercised here without a separate update - and runs them for
//! real under `node`. `workerLabel` (and the `ROSTER_VERB` table it now reads alongside
//! `PERSONA_VERB`) depends on no injected global, so nothing needs stubbing.

use std::path::Path;
use std::process::Command;

/// Extract a top-level declaration - `function <name>(...) { ... }` or `const <NAME> = { ... }` -
/// VERBATIM from `start_marker` through its brace-matched close, inclusive. This file's own copy
/// of the same brace-counting helper `tests/worker_persona_label_periphery.rs::js_declaration`
/// and `tests/step_attention_periphery.rs::js_declaration` carry, per the established per-file
/// duplication convention.
fn js_declaration<'a>(src: &'a str, start_marker: &str) -> &'a str {
    let start = src
        .find(start_marker)
        .unwrap_or_else(|| panic!("workflow must contain `{start_marker}`"));
    let open = start
        + src[start..]
            .find('{')
            .expect("declaration must open a brace");
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[start..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("`{start_marker}` is not brace-balanced");
}

/// Read `workflows/rigger.js` at test time from the crate manifest dir - mirrors this crate's
/// other `rigger_js_source` helpers.
fn rigger_js_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workflows")
        .join("rigger.js");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

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
    let src = rigger_js_source();
    let verb_table = js_declaration(&src, "const PERSONA_VERB = {");
    let roster_table = js_declaration(&src, "const ROSTER_VERB = {");
    let persona_of = js_declaration(&src, "function personaOf(role) {");
    let first_sentence = js_declaration(&src, "function firstSentence(s) {");
    let role_attempt = js_declaration(&src, "function roleAttempt(id) {");
    let worker_label = js_declaration(&src, "function workerLabel(req) {");

    let mut req = serde_json::json!({ "id": id, "title": title });
    if let Some(reviews) = reviews {
        req["reviews"] = serde_json::Value::Array(
            reviews
                .iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect(),
        );
    }
    let req = req.to_string();
    let script = format!(
        "{verb_table}\n{roster_table}\n{persona_of}\n{first_sentence}\n{role_attempt}\n\
         {worker_label}\nprocess.stdout.write(workerLabel(JSON.parse(process.argv[2])) + '\\n')\n"
    );

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut f, script.as_bytes()).unwrap();

    match Command::new(&node).arg(f.path()).arg(&req).output() {
        Ok(out) => {
            assert!(
                out.status.success(),
                "the real workerLabel must run without throwing on {req}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            Some(text.trim_end_matches('\n').to_string())
        }
        Err(e) => {
            assert!(
                e.kind() == std::io::ErrorKind::NotFound,
                "node failed for a reason other than being absent: {e}"
            );
            None
        }
    }
}

/// The adversary's action phrase inlines the routed roster the conductor stamped: "challenge
/// the findings, assumptions, and rigor of <roster>" - the roster is the conductor's stamped
/// `lens:<id>` tokens, joined verbatim, never re-derived or re-cased by the driver.
#[test]
fn the_adversarys_roster_renders_inside_its_action_phrase() {
    let Some(label) = run_worker_label_with_reviews(
        "u1/adversary#0",
        "challenge the lenses' findings.",
        Some(&["lens:sdet", "lens:architecture-reviewer"]),
    ) else {
        return; // node unavailable; graceful absence.
    };
    assert_eq!(
        label,
        "Adversary - challenge the findings, assumptions, and rigor of lens:sdet, \
         lens:architecture-reviewer #0: challenge the lenses' findings."
    );
}

/// The adjudicator's action phrase inlines lenses PLUS the adversary: "weigh <roster> and
/// rule" - proving the roster is inserted INSIDE the phrase (between "weigh" and "and rule"),
/// not merely appended after it.
#[test]
fn the_adjudicators_roster_renders_inside_its_action_phrase() {
    let Some(label) = run_worker_label_with_reviews(
        "u1/adjudicator#1",
        "weigh the lenses and the adversary.",
        Some(&["lens:sdet", "adversary"]),
    ) else {
        return;
    };
    assert_eq!(
        label,
        "Adjudicator - weigh lens:sdet, adversary and rule #1: weigh the lenses and the \
         adversary."
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

/// A roster on a role with NO `ROSTER_VERB` entry (a lens, or any non-review-tier role) is
/// NEVER rendered - "never a fabricated or stale roster" cuts both ways: the driver must not
/// invent a roster clause for a role the conductor never intends one for, even if `req.reviews`
/// happened to carry a (malformed) value.
#[test]
fn a_roster_on_a_role_with_no_roster_verb_entry_is_never_rendered() {
    let Some(label) = run_worker_label_with_reviews(
        "u4/lens:sdet#0",
        "evaluate whether the tests are discriminating.",
        Some(&["lens:sdet"]),
    ) else {
        return;
    };
    assert_eq!(
        label,
        "Lens:SDET - evaluate testing effectiveness #0: evaluate whether the tests are \
         discriminating."
    );
}

/// A single-entry roster renders with no stray separator - proving the join is exercised at
/// both cardinalities, not merely assumed correct from the two-entry cases above.
#[test]
fn a_single_entry_roster_renders_with_no_stray_separator() {
    let Some(label) = run_worker_label_with_reviews(
        "u5/adversary#2",
        "challenge the one lens.",
        Some(&["lens:sdet"]),
    ) else {
        return;
    };
    assert_eq!(
        label,
        "Adversary - challenge the findings, assumptions, and rigor of lens:sdet #2: \
         challenge the one lens."
    );
}
