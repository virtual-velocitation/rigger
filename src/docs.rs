//! The self-documenting discipline pipeline (spec 20, unit 1).
//!
//! The operating discipline - when to reach for the loop, the one blessed driver,
//! spec shape, base anchoring, the verdict contract, fix-the-loop-when-it-wedges,
//! and auto-integration on approve - is rendered from ONE typed context so the
//! document cannot silently disagree with the code the binary runs on. Two kinds of
//! content are kept apart so the whole document stays accurate:
//!
//! - PROSE (the WHY, the rationales) lives in the hand-authored template functions
//!   below, because prose cannot be inferred from code.
//! - FACTS (every value that could drift - the default base ref, the dashboard port,
//!   the remediation bound, the verdict-line literal, the spec-shape rules, the
//!   command surface) are carried on [`DocsContext`] and interpolated by typed field
//!   access. A template that references a fact the code no longer exposes fails to
//!   COMPILE (the template is checked against the context type at build time), so a
//!   fact cannot silently diverge from behavior - and there is no external toolchain
//!   that would require re-exporting the facts.
//!
//! The composition root (the binary) populates the context from the real code
//! definitions and calls [`render_using_rigger_skill`] and
//! [`render_handbook_discipline`]. Both outputs render from the SAME context, so a
//! project overlay that overrides a field (unit 3) flows into both through this one
//! pipeline. The render is byte-stable on unchanged inputs (no map iteration, fixed
//! collection order), so the drift check (unit 2) has no false positives.

use std::fmt::Write as _;

/// The typed, code-derived facts the discipline templates interpolate. Every field is
/// a value that could drift from behavior if hand-copied, so the composition root
/// populates each one FROM the code definition the runtime uses (see `docs_context` in
/// the binary). A project overlay merges by overriding fields here BEFORE rendering, so
/// repo specifics and the shared discipline share this one pipeline.
///
/// Removing or renaming a field breaks every template that interpolates it at COMPILE
/// time - that is the load-bearing property: the templates are validated against this
/// type by the build, not by a runtime check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocsContext {
    /// The default ref a run anchors its branch on (`DEFAULT_BASE_REF`).
    pub base_ref: String,
    /// The loopback port the always-on dashboard binds first (`dash::DEFAULT_PORT`).
    pub dash_port: u16,
    /// The bounded-remediation ceiling before a unit escalates (`safety::MAX_RETRIES`).
    pub max_retries: u32,
    /// The verdict value that approves a unit on the result channel, read from the same
    /// definition the integration gate uses (`conductor::VERDICT_APPROVE`).
    pub verdict_approve: String,
    /// The spec-shape lint rule names, in document order (`spec::ShapeRule`).
    pub spec_shape_rules: Vec<String>,
    /// The single recommendation every spec-shape advisory ends with
    /// (`spec::SHAPE_RECOMMENDATION`).
    pub spec_shape_recommendation: String,
    /// The command surface, in registry order (the `SUBCOMMANDS` dispatch registry).
    pub subcommands: Vec<String>,
    /// Where this repo keeps its specs. A project-overlay override point (unit 3);
    /// defaults to the shared convention.
    pub specs_location: String,
}

/// Render the `using-rigger` skill: a self-contained front-door that tells an agent
/// WHEN and HOW to drive the loop. Distinct from the `/rigger` workflow (which RUNS the
/// loop); this skill tells an agent when to reach for it and how to stay on the rails.
///
/// The skill opens with loadable frontmatter (a name and a description) so it installs
/// as a discoverable skill, then carries the shared discipline body. Every drift-prone
/// value comes from `ctx`.
pub fn render_using_rigger_skill(ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str("name: using-rigger\n");
    s.push_str(
        "description: When and how to drive rigger - the one blessed driver, spec shape, \
         base anchoring, the verdict contract, and fix-the-loop discipline. Read this before \
         starting or driving a rigger run.\n",
    );
    s.push_str("---\n\n");
    s.push_str("# Using rigger\n\n");
    s.push_str(
        "This is the front-door for driving a rigger run: when to reach for the loop, the \
         one blessed way to drive it, and the rails that keep a run honest. It is generated \
         from the code the binary runs on, so its facts cannot drift from behavior.\n\n",
    );
    s.push_str(&discipline_body(ctx));
    s
}

/// Render the handbook's discipline chapter from the SAME context as the skill, so the
/// two never disagree. The chapter is the operator-manual framing of the discipline.
pub fn render_handbook_discipline(ctx: &DocsContext) -> String {
    let mut s = String::new();
    s.push_str("# Using rigger: the operating discipline\n\n");
    s.push_str(
        "This chapter is the operating discipline for a rigger run: when the loop is the \
         right tool, the one blessed driver, and the rails that keep a run consistent. Its \
         facts are generated from the code the binary runs on, so the chapter cannot silently \
         disagree with how rigger actually behaves.\n\n",
    );
    s.push_str(&discipline_body(ctx));
    s
}

/// The shared discipline body both outputs carry, so the skill and the handbook chapter
/// render from ONE context and can never disagree. Every fact is interpolated from
/// `ctx` by typed field access, so a fact the code stops exposing breaks this at compile
/// time. Written pure-ASCII (hyphens, not unicode dashes) so the drift check has no
/// false positives, and self-contained (it explains the problem each rule solves and
/// names no tool outside rigger's own surface).
fn discipline_body(ctx: &DocsContext) -> String {
    let mut s = String::new();

    let _ = writeln!(s, "## When to reach for rigger\n");
    let _ = writeln!(
        s,
        "Reach for rigger when you have a written spec whose \"Done when\" section \
         enumerates machine-checkable criteria and you want it built, tested, reviewed, and \
         integrated without hand-holding each step. Do NOT reach for it for a one-line edit, \
         an exploratory spike, or work that has no spec to anchor acceptance on - the loop's \
         value is the disciplined lifecycle around a checkable spec, and without one there is \
         nothing for it to hold to.\n"
    );

    let _ = writeln!(s, "## The one blessed driver\n");
    let _ = writeln!(
        s,
        "Drive every run through the native /rigger workflow (visible in /workflows and on \
         the dashboard at 127.0.0.1:{port}). It launches the loop and keeps the event log, the \
         ledger, and the context graph consistent with one another. These anti-patterns split \
         the run's state away from that shared record and must be avoided:\n",
        port = ctx.dash_port
    );
    let _ = writeln!(
        s,
        "- Polling git or ps by hand to guess progress. Read the dashboard or `rigger \
         status`; the by-hand view misses the ledger and the graph."
    );
    let _ = writeln!(
        s,
        "- Hand-driving `rigger step` in a shell. The driver owns stepping; a hand step \
         races the driver and can double-spawn or wedge the frontier."
    );
    let _ = writeln!(
        s,
        "- Hand-implementing a unit the loop parked. That leaves the loop still stuck for \
         the next unit and forks the code from the log - fix the loop instead (see below).\n"
    );

    let _ = writeln!(s, "## Spec shape\n");
    let _ = writeln!(
        s,
        "One observable behavior per criterion; the atomic unit is one checkbox; put type \
         shapes and structural detail in a non-criteria Notes section. The loop's spec-shape \
         lint flags these shapes because a planner paraphrases or truncates them when told to \
         copy a criterion verbatim, which then fails the baseline match the conductor \
         reconciles proposals against: {rules}. Recommendation: {rec}.\n",
        rules = ctx.spec_shape_rules.join(", "),
        rec = ctx.spec_shape_recommendation
    );

    let _ = writeln!(s, "## Base anchoring\n");
    let _ = writeln!(
        s,
        "A run anchors its branch on the working ref (default {base}) and reuses that \
         branch once it exists. Anchor on the ref you actually want the work to land on, not \
         a stale default: the anchor is what every unit worktree branches from and every \
         approved unit merges back into, so an anchor on the wrong ref lands the run in the \
         wrong place.\n",
        base = ctx.base_ref
    );

    let _ = writeln!(s, "## When it wedges, fix the loop\n");
    let _ = writeln!(
        s,
        "If a unit will not pass, the fix belongs in the loop - the spec, the gate, the \
         agent, or the config - never a manual edit that sidesteps it. A by-hand fix leaves \
         the loop broken for the next unit and splits the code from the log, so the run can no \
         longer be trusted to replay. Correct the underlying cause and let the loop re-run \
         the unit.\n"
    );

    let _ = writeln!(s, "## Auto-integration on approve\n");
    let _ = writeln!(
        s,
        "An approved unit integrates itself onto the run branch. A human reviews the whole \
         run by opening a pull request FROM the run branch, never by cherry-picking approved \
         units by hand - cherry-picking drops the run's accumulated context and its ordering. \
         A failing unit is retried under a bounded budget (up to {max} attempts) and then \
         escalated to a human rather than spinning forever.\n",
        max = ctx.max_retries
    );

    let _ = writeln!(s, "## The verdict line\n");
    let _ = writeln!(
        s,
        "Every gating agent ends its output with its verdict line: a JSON line carrying \
         {{\"verdict\":\"{verdict}\"}} to approve (or the rejecting value to send the unit \
         back). The integration gate reads that result line, not events recorded through any \
         side channel, so an agent that records its verdict only out-of-band returns no \
         verdict the gate can see and stalls the run. Anyone authoring or porting a gating \
         persona must keep this line.\n",
        verdict = ctx.verdict_approve
    );

    let _ = writeln!(s, "## Self-serve\n");
    let _ = writeln!(
        s,
        "Run `rigger version` to see the exact binary and its build provenance and to \
         diagnose drift between the installed /rigger workflow and the binary that would run \
         it. This repo keeps its specs in {specs}. The full command surface is: {cmds}.\n",
        specs = ctx.specs_location,
        cmds = ctx.subcommands.join(", ")
    );

    let _ = writeln!(s, "## The load-bearing decisions\n");
    let _ = writeln!(s, "The discipline explains its own constraints:\n");
    let _ = writeln!(
        s,
        "- One source of truth: every drift-prone fact in this document is read from the \
         code the binary runs on, so the document cannot silently disagree with behavior. A \
         drift check re-renders and diffs it, so it stays accurate rather than merely starting \
         accurate."
    );
    let _ = writeln!(
        s,
        "- Blast-radius isolation: each unit does its work in its own worktree, so \
         concurrent units never clobber one another and every unit's change is reviewed on its \
         own diff."
    );
    let _ = writeln!(
        s,
        "- Fail-closed review: only an explicit approve verdict integrates a unit; a \
         missing, unparseable, or rejecting verdict routes the unit back to remediation rather \
         than passing it silently."
    );

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic context with sentinel values so a rendered output that reflects them
    /// proves the render is PARAMETERIZED by the context, not hardcoded.
    fn sentinel_ctx() -> DocsContext {
        DocsContext {
            base_ref: "SENTINEL/base-ref".to_string(),
            dash_port: 65531,
            max_retries: 999,
            verdict_approve: "sentinelverdict".to_string(),
            spec_shape_rules: vec!["sentinel-rule-a".to_string(), "sentinel-rule-b".to_string()],
            spec_shape_recommendation: "sentinel recommendation text".to_string(),
            subcommands: vec!["sentinelcmd-a".to_string(), "sentinelcmd-b".to_string()],
            specs_location: "sentinel-specs/".to_string(),
        }
    }

    #[test]
    fn skill_render_is_parameterized_by_every_fact() {
        let ctx = sentinel_ctx();
        let out = render_using_rigger_skill(&ctx);
        assert!(out.contains("SENTINEL/base-ref"), "base_ref not rendered");
        assert!(out.contains("65531"), "dash_port not rendered");
        assert!(out.contains("999"), "max_retries not rendered");
        assert!(
            out.contains("sentinelverdict"),
            "verdict_approve not rendered"
        );
        assert!(
            out.contains("sentinel-rule-a"),
            "spec_shape_rule not rendered"
        );
        assert!(
            out.contains("sentinel recommendation text"),
            "spec_shape_recommendation not rendered"
        );
        assert!(out.contains("sentinelcmd-a"), "subcommand not rendered");
    }

    #[test]
    fn handbook_render_is_parameterized_by_every_fact() {
        let ctx = sentinel_ctx();
        let out = render_handbook_discipline(&ctx);
        assert!(out.contains("SENTINEL/base-ref"), "base_ref not rendered");
        assert!(out.contains("65531"), "dash_port not rendered");
        assert!(out.contains("999"), "max_retries not rendered");
        assert!(
            out.contains("sentinelverdict"),
            "verdict_approve not rendered"
        );
        assert!(
            out.contains("sentinel-rule-a"),
            "spec_shape_rule not rendered"
        );
        assert!(out.contains("sentinelcmd-a"), "subcommand not rendered");
    }

    #[test]
    fn both_outputs_render_from_the_one_context() {
        // A fact set only on the context appears in BOTH outputs, proving one context
        // feeds both renders (no second, drifting source).
        let ctx = sentinel_ctx();
        assert!(render_using_rigger_skill(&ctx).contains("SENTINEL/base-ref"));
        assert!(render_handbook_discipline(&ctx).contains("SENTINEL/base-ref"));
    }

    #[test]
    fn skill_carries_claude_code_skill_frontmatter() {
        let out = render_using_rigger_skill(&sentinel_ctx());
        assert!(
            out.starts_with("---\nname: using-rigger\n"),
            "the skill must open with skill frontmatter naming it; got: {}",
            &out[..out.len().min(80)]
        );
        assert!(
            out.contains("\ndescription: "),
            "frontmatter needs a description"
        );
    }

    #[test]
    fn render_is_byte_stable_across_runs() {
        let ctx = sentinel_ctx();
        assert_eq!(
            render_using_rigger_skill(&ctx),
            render_using_rigger_skill(&ctx)
        );
        assert_eq!(
            render_handbook_discipline(&ctx),
            render_handbook_discipline(&ctx)
        );
    }

    #[test]
    fn render_carries_no_unicode_dashes() {
        // The drift check has no false positives only if the render is pure ASCII dashes
        // (the diff gate fails on U+2014 and the other unicode dashes).
        let ctx = sentinel_ctx();
        for out in [
            render_using_rigger_skill(&ctx),
            render_handbook_discipline(&ctx),
        ] {
            for bad in ['\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}'] {
                assert!(
                    !out.contains(bad),
                    "render must not contain unicode dash {bad:?}"
                );
            }
        }
    }
}
