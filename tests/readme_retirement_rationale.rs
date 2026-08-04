//! Spec 57, criterion 4 - the README TELLS THE NEW TRUTH about grounding.
//!
//! The front-door `README.md` is the "why" of the project. Spec 57 retired the
//! vector-embedding grounder: the `symbols` (structural) grounder is the DEFAULT, the vector
//! engine and its dependency tree leave the build, and the knowledge graph is the lookup
//! surface. The criterion requires the README to record that truth WITH the measured
//! rationale, so the retirement decision is legible to every consumer and a later edit that
//! quietly reinstates a "grep (default)" / "turbovec upgrade" story fails RED at `cargo test`
//! time instead of shipping a README that contradicts the code.
//!
//! Two properties are pinned:
//!
//!   1. The README NAMES the new default and carries the RETIREMENT RATIONALE with the
//!      MEASURED NUMBERS - the two independent measurements that answered the retention
//!      question (an identical retrieval hit-set / zero marginal recall, and zero invocations
//!      across BOTH runs of the A/B workload) and the two costs the removal sheds (the
//!      per-step index freshen and the per-build/per-install dependency tree).
//!   2. The README carries NONE of the retired inversions - it must not call `grep` the
//!      default grounder, must not present the vector engine as a build-time "semantic
//!      grounding" upgrade, and must not name the retired `turbovec` engine as a live choice.
//!
//! Like the `architecture.md` pins, this test reads the committed `README.md` (resolved from
//! `CARGO_MANIFEST_DIR`, so it does not depend on the process CWD), parses a text file, and
//! touches no backend symbol. It is deliberately NOT feature-gated: it runs identically in
//! both feature lanes.

use std::path::PathBuf;

/// The committed front-door README, resolved from the manifest dir so the test does not
/// depend on the process CWD (integration tests may run from anywhere).
fn readme_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("README.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every (fact, needle) the README must carry to tell the new grounding truth. Each needle is
/// a lowercased literal the retirement rationale states, so a README that drops the default
/// claim or any measured number fails loudly, naming exactly which fact went missing.
const REQUIRED_TRUTHS: &[(&str, &str)] = &[
    // The new default is the structural symbols grounder.
    (
        "names the structural `symbols` grounder as the default",
        "default is the `symbols` grounder",
    ),
    // Measurement 1: the vector index added no recall over the graph's structural retrieval.
    (
        "records the identical-hit-set measurement (vs the graph's structural retrieval)",
        "identical to the knowledge graph",
    ),
    (
        "records the zero-marginal-recall result",
        "zero marginal recall",
    ),
    // Measurement 2: in the A/B workload the vector surface was used by no agent in either run.
    (
        "records that the vector surface was invoked zero times",
        "invoked zero times",
    ),
    (
        "records that the zero-invocation result held across both A/B runs",
        "both runs",
    ),
    // The two costs the removal sheds: the per-step freshen and the per-build/per-install deps.
    (
        "records the per-step index-freshen cost the removal sheds",
        "taxed every step",
    ),
    (
        "records the per-build and per-install dependency cost the removal sheds",
        "every build and install",
    ),
    // The retirement itself is stated, not merely implied.
    ("states the grounder was retired", "retired"),
];

/// Phrasings that tell the OLD, now-inverted grounding story: `grep` as the default, or the
/// retired vector engine offered as a build-time "semantic grounding" upgrade. None may
/// survive in the README after the retirement (spec 57). Checked case-insensitively.
const RETIRED_INVERSIONS: &[&str] = &[
    // grep is the explicit opt-out, never the default grounder.
    "grep grounder",
    // the retired vector engine is not a build-time upgrade to opt into.
    "--features turbovec",
    "for semantic grounding",
    "turbovec engine",
];

#[test]
fn readme_records_the_symbols_default_and_the_retirement_rationale() {
    let text = readme_text().to_lowercase();

    let missing: Vec<String> = REQUIRED_TRUTHS
        .iter()
        .filter(|(_, needle)| !text.contains(needle))
        .map(|(fact, needle)| format!("{fact}  (missing: {needle:?})"))
        .collect();

    assert!(
        missing.is_empty(),
        "README.md must tell the new grounding truth (spec 57, criterion 4): name the \
         structural `symbols` grounder as the default AND carry the retirement rationale with \
         the measured numbers - the identical retrieval hit-set / zero marginal recall, the \
         zero invocations across both A/B workload runs, and the per-step freshen plus \
         per-build/per-install dependency costs the removal sheds. Truths the README fails to \
         record: {missing:#?}"
    );
}

#[test]
fn readme_carries_none_of_the_retired_grounder_inversions() {
    let text = readme_text().to_lowercase();

    let inversions: Vec<&str> = RETIRED_INVERSIONS
        .iter()
        .copied()
        .filter(|phrasing| text.contains(phrasing))
        .collect();

    assert!(
        inversions.is_empty(),
        "README.md must not tell the retired grounding story (spec 57, criterion 4): `grep` is \
         the explicit opt-out, never the default, and the retired vector engine is not a \
         build-time `semantic grounding` upgrade to opt into. Inverting phrasings still present \
         in the README: {inversions:#?}"
    );
}
