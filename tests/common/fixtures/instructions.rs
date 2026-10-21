//! Built-in instruction fixtures: the working-discipline body's section headings and the review
//! method its `## When you review` section holds, shared by the instruction layer's unit tests, the
//! conductor's reviewer-prompt tests, the binary's shipped-text tests and the integration suites.

/// The working-discipline body's hand-over section heading.
pub const HAND_OVER_HEADING: &str = "## Hand over a tree the gates only confirm";

/// The working-discipline body's review section heading.
pub const WHEN_YOU_REVIEW_HEADING: &str = "## When you review";

/// The working-discipline body's fan-out section heading.
pub const FAN_OUT_HEADING: &str = "## Fan out the mechanical work";

/// The three review sentences the working-discipline body's `## When you review` section holds,
/// in its order: the review method a reviewer prompt's discipline no longer carries.
pub const WHEN_YOU_REVIEW: [&str; 3] = [
    "Never re-run the gate battery, or any gate the gate evidence shows green on the tree under \
     review: the gates are the instrument that grades the work, and a review judges what they \
     cannot see.",
    "To reproduce a specific suspicion - one named test, a reversion probe, a single targeted test \
     run through the project's test command - create your own throwaway scratch worktree and run \
     it there.",
    "Never run a gate's command as the gate runs it, directly or through a verify helper: the gate \
     evidence in your prompt is that gate's verdict, and a reviewer's run only repeats it at review \
     cost; the one targeted reproduction above is not a gate run.",
];

/// The review `tier`'s task `prompt` carries the reviewer `discipline` exactly once and none of the
/// review method `## When you review` holds, the method reaching the tier through its system prompt.
pub fn assert_task_prompt_carries_only_the_reviewer_contract(
    tier: &str,
    prompt: &str,
    discipline: &str,
) {
    assert_eq!(
        prompt.matches(discipline).count(),
        1,
        "the {tier} task prompt carries the reviewer discipline exactly once; prompt was:\n{prompt}"
    );
    for sentence in WHEN_YOU_REVIEW {
        assert_eq!(
            prompt.matches(sentence).count(),
            0,
            "the {tier} task prompt leaves {sentence:?} to `## When you review`; prompt \
             was:\n{prompt}"
        );
    }
}

/// The review `tier`'s whitespace-normalized `system` prompt carries each review sentence
/// `## When you review` holds exactly once.
pub fn assert_system_prompt_carries_the_review_method_once(tier: &str, system: &str) {
    for sentence in WHEN_YOU_REVIEW {
        assert_eq!(
            system.matches(sentence).count(),
            1,
            "the {tier} system prompt carries {sentence:?} once; system prompt \
             was:\n{system}"
        );
    }
}
