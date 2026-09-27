//! Periphery (integration) test for spec 61 criterion 1 (TOLERANT ATTRIBUTION), unit-1:
//! a tier catches when it raises a finding about the anchor under TOLERANT path spelling -
//! `Finding::catches`/`paths_match` (src/canary.rs) score the catch when a reviewer's
//! `about` entry and the item's `anchor` name the same file at a path-segment boundary.
//!
//! The implementer's own inside-out unit tests pin `paths_match`/`catches` directly, but
//! every one of them gives `Finding.about` a SINGLE-element slice - none proves `.any()`
//! actually searches every entry of a real reviewer's multi-file `about` list (a live
//! reviewer commonly cites several files in one finding, per the `about: [<file>, ...]`
//! schema) rather than only ever happening to see the match land first. None guards an
//! individual `about` ENTRY being empty either - only `catches` guards the item's `anchor`
//! being empty, leaving `paths_match` itself exposed to a real inversion: `str::strip_suffix`
//! on an EMPTY needle trivially succeeds on any haystack, so `paths_match("", anchor)`
//! returned `true` whenever `anchor` happened to end in a path separator - an empty `about`
//! entry (a live reviewer's malformed finding, or a corpus author's anchor typo landing on
//! a bare directory path) scored a spurious catch bearing zero relation to what was actually
//! found, the opposite of the honest-attribution guarantee this criterion exists to deliver.
//!
//! This suite drives `run_canary` - the public entry the shipped `rigger canary` command
//! calls - end to end through a scripted driver, over corpus items and findings shaped
//! exactly like a live reviewer's, proving both boundary cases neither the implementer's
//! own unit tests nor the periphery layer previously covered.

mod common;

use common::fixtures::cfg_for;
use common::fixtures::lens_only_panel;
use rigger::canary::TIER_LENS;
use rigger::canary_store::{default_jobs, run_canary, CanaryItem};
use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::AgentDef;
use rigger::contextgraph::TYPE_REVIEW_FINDING;
use rigger::eventstore::sqlite::Store;
use serde_json::{json, Value};

/// A scripted driver: the lone lens `"lens"` raises a FIXED finding, chosen by which item
/// id is under review (identified from the review prompt, which embeds the item id as its
/// corpus header). The adversary and adjudicator are inert - the adjudicator always
/// approves, since this suite scores only the lens tier's `caught_by` attribution, not the
/// gating verdict.
struct AttributionDriver;

impl AgentDriver for AttributionDriver {
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
        if a.id == "lens" {
            if prompt.contains("multi-about") {
                // The tolerant-matching entry is the SECOND element of a multi-file
                // `about` list, preceded by an unrelated file - the shape a real
                // reviewer's finding takes when it cites several files at once.
                emit(
                    TYPE_REVIEW_FINDING,
                    json!({
                        "id": "f-multi",
                        "by": "lens",
                        "summary": "defect here",
                        "about": ["unrelated.rs", "/home/dev/repo/src/sum.rs"],
                    }),
                )?;
            } else if prompt.contains("empty-about-trap") {
                // An `about` entry that is the EMPTY string, alongside a genuinely
                // unrelated file - the malformed-finding / anchor-typo shape that must
                // never, on its own, score a catch.
                emit(
                    TYPE_REVIEW_FINDING,
                    json!({
                        "id": "f-empty",
                        "by": "lens",
                        "summary": "defect here",
                        "about": ["", "totally-unrelated.rs"],
                    }),
                )?;
            }
        }
        Ok(AgentResult {
            output: "reviewed".into(),
            resolved_model: String::new(),
        })
    }
}

/// The agents every panel in this file names.
const PANEL_AGENTS: &[&str] = &["lens", "adv", "adj"];

fn item(id: &str, anchor: &str) -> CanaryItem {
    CanaryItem {
        id: id.into(),
        defect_class: "off-by-one".into(),
        planted: true,
        anchor: anchor.into(),
        expected_verdict: "reject".into(),
        expected_tier: "lens".into(),
        review: format!("fn {id}() {{}}"),
    }
}

/// Runs the canary through its public entry over ONE item planted at `anchor`, reviewed by the
/// lens-only panel whose [`AttributionDriver`] raises its fixed finding for `id`: the one outcome
/// is caught by exactly the `caught_by` tiers (`why`).
fn assert_caught_by(id: &str, anchor: &str, caught_by: &[&str], why: &str) {
    let store = Store::open(":memory:").expect("an in-memory store opens");
    let corpus = vec![item(id, anchor)];

    let report = run_canary(
        &store,
        &AttributionDriver,
        &cfg_for(PANEL_AGENTS),
        &lens_only_panel(),
        &corpus,
        default_jobs(),
        &|_, _| {},
    )
    .expect("run_canary succeeds through the public entry");

    assert_eq!(report.outcomes.len(), 1);
    assert_eq!(report.outcomes[0].caught_by, caught_by, "{why}");
}

rigger::test_cases! {
    /// A tolerant match anywhere in a multi-entry `about` list scores the catch - not only when
    /// the match happens to be the list's first or only entry. Drives `run_canary` with a
    /// finding whose `about` is `["unrelated.rs", "/home/dev/repo/src/sum.rs"]` against anchor
    /// `"src/sum.rs"`: the match is the SECOND entry, behind an unrelated file that must not
    /// short-circuit the search.
    a_tolerant_match_in_a_later_about_entry_still_scores_the_catch: assert_caught_by(
        "multi-about",
        "src/sum.rs",
        &[TIER_LENS],
        "the finding's second about-entry ('/home/dev/repo/src/sum.rs') is a tolerant \
         spelling of the anchor 'src/sum.rs' - the lens tier must catch this even though \
         an unrelated file precedes it in the same finding's about list, proving the catch \
         search does not stop at (or require) the first entry",
    );
    /// An EMPTY `about` entry must never score a catch on its own - not even against an anchor
    /// ending in a path separator, where `paths_match`'s suffix-stripping previously inverted
    /// an absent value into a spurious match (`str::strip_suffix("")` trivially succeeds on any
    /// haystack). Drives `run_canary` with a finding whose `about` is
    /// `["", "totally-unrelated.rs"]` against a trailing-slash anchor (`parse_item` applies no
    /// shape validation to a corpus author's `anchor:` field, so this is a reachable real
    /// shape, not a contrived one) - neither entry names the anchor, so the lens tier must not
    /// catch it.
    an_empty_about_entry_never_scores_a_catch_even_against_a_trailing_slash_anchor:
        assert_caught_by(
            "empty-about-trap",
            "corpus/",
            &[],
            "neither '' nor 'totally-unrelated.rs' names the anchor 'corpus/' - an empty \
             about entry must never invert into a spurious catch just because the anchor \
             ends in a path separator",
        );
}
