//! The review-verdict logic: the fail-closed verdict-line reading on the result channel, the
//! risk-tiered review-depth routing, and the review rosters.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::spawn::{lens_role, ROLE_ADVERSARY};

/// The two review-depth tiers a unit routes to: `TIER_LIGHT` runs the reduced roster,
/// `TIER_FULL` the whole panel (spec 03 / spec 13 unit 4).
pub const TIER_LIGHT: &str = "light";
pub const TIER_FULL: &str = "full";

/// Match a glob `pattern` against a repo-relative `path` (spec 12, unit 3). `**` matches any
/// run of characters INCLUDING `/` (any number of path segments, and a trailing `/` is
/// optional so `a/**/b` also matches `a/b`); `*` matches any run EXCLUDING `/` (one path
/// segment); `?` matches a single non-`/` character; every other character is literal. Built
/// by translating the glob to an anchored regex; a malformed translation matches nothing.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    let mut re = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    re.push_str(".*");
                    // Swallow the separator after `**` so `a/**/b` matches `a/b` (zero dirs).
                    if chars.peek() == Some(&'/') {
                        chars.next();
                    }
                } else {
                    re.push_str("[^/]*");
                }
            }
            '?' => re.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            _ => re.push(c),
        }
    }
    re.push('$');
    regex::Regex::new(&re)
        .map(|r| r.is_match(path))
        .unwrap_or(false)
}

/// Whether a `high_risk_paths` entry matches a blast-radius file (spec 03 / spec 13
/// unit 4). An entry matches by literal path PREFIX (so `src/` or `src/conductor`
/// flags `src/conductor.rs`) OR by the same glob semantics gate `inputs:` use (so
/// `specs/**` or `src/*.rs` work). Either match forces the FULL review panel even for
/// a small change - the "core trait / spec file" escape hatch the size threshold alone
/// would miss.
pub fn path_is_high_risk(pattern: &str, file: &str) -> bool {
    file.starts_with(pattern) || glob_matches(pattern, file)
}

/// The review tier a unit was routed to and the OBSERVABLE inputs that decided it
/// (spec 03 "adaptive review depth", spec 13 unit 4). Returned by [`route_review_tier`]
/// so `review_unit` can both run the chosen panel and LOG the routing decision with its
/// inputs ("every routing decision logged with its inputs").
pub struct TierRouting<'a> {
    /// The panel to run - the reduced light roster or the full panel.
    pub panel: &'a crate::config::ReviewPanel,
    /// [`TIER_LIGHT`] or [`TIER_FULL`].
    pub tier: &'static str,
    /// The unit's grounded blast-radius file count (the size signal).
    pub blast_radius: usize,
    /// The low-risk size threshold in effect (`0` when no policy is configured).
    pub threshold: usize,
    /// The first blast-radius file that matched a `high_risk_paths` entry, if any.
    pub high_risk_hit: Option<String>,
    /// Whether the grounded blast radius was EMPTY - no grounder configured, or a
    /// coverage query that grounded to zero files. An absent risk signal is
    /// UNASSESSABLE, so it fails SAFE to the FULL panel (never LIGHT): the size and
    /// high-risk-path signals can say nothing about a radius with no files, and a
    /// tiers-without-a-grounder workflow must not silently downgrade every unit to
    /// light. Recorded so the routing log shows WHY the full panel ran.
    pub empty_radius: bool,
    /// Whether the unit's gates FLAPPED - it needed remediation to reach green (the
    /// spec-13 "gate outcome" signal, observed as `attempt > 0`).
    pub flapped: bool,
    /// Whether a depth policy was configured at all. `false` means every unit runs the
    /// full panel unchanged and NOTHING is logged - the shipped default.
    pub policy: bool,
}

/// Route a unit's review to the LIGHT or FULL panel by its observable risk (spec 03
/// "adaptive review depth", spec 13 unit 4 "risk-tiered review depth"). `full` is the
/// unit's effective panel; when it carries no `tiers` depth policy every unit runs it
/// unchanged (`policy: false`). Otherwise the unit runs the FULL panel if ANY high-risk
/// signal holds - a blast-radius file matches a high-risk path, the blast-radius file
/// count EXCEEDS the threshold, the gates flapped, or the blast radius is EMPTY - and the
/// reduced LIGHT panel only when none of them do. The empty-radius arm is the FAIL-SAFE
/// default: an absent risk signal (no grounder configured, or a coverage query that
/// grounds to zero files) is unassessable, so it routes to FULL rather than LIGHT - the
/// size and high-risk-path signals can prove nothing about a radius with no files, and a
/// tiers-without-a-grounder workflow must never silently downgrade EVERY unit to light
/// (the safety mechanism fails SAFE, not OPEN). The size signal `blast_radius` is the
/// unit's `.safe` structural blast-radius view (spec 16 unit 3): on the STRUCTURAL
/// grounder it is the UNCAPPED structural-width superset, so `threshold` is a LIVE gate
/// over the change's true width and a `threshold >= 8` is NOT inert - any wider change
/// routes to the full panel; on the default / grep lane `.safe` equals the capped
/// grounded seed, so the threshold behaves exactly as it did before unit 3 (tune it to
/// the structural-width distribution - see
/// [`ReviewDepth::threshold`](crate::config::ReviewDepth::threshold)).
/// The adjudicator and the full gate suite stay mandatory on every tier
/// (config validation forces both the light AND the full panel to name an adjudicator; the
/// gates run outside the review), so a light-routed unit still gets its gating verdict -
/// only the adversary and the extra lenses flex. Pure over the panel + signals, so it is
/// unit-tested directly.
pub fn route_review_tier<'a>(
    full: &'a crate::config::ReviewPanel,
    blast_radius: &[String],
    flapped: bool,
) -> TierRouting<'a> {
    let depth = match full.depth() {
        None => {
            return TierRouting {
                panel: full,
                tier: TIER_FULL,
                blast_radius: blast_radius.len(),
                threshold: 0,
                high_risk_hit: None,
                empty_radius: blast_radius.is_empty(),
                flapped,
                policy: false,
            };
        }
        Some(d) => d,
    };
    let high_risk_hit = blast_radius
        .iter()
        .find(|f| {
            depth
                .high_risk_paths
                .iter()
                .any(|p| path_is_high_risk(p, f))
        })
        .cloned();
    let over_threshold = blast_radius.len() > depth.threshold;
    // An EMPTY grounded blast radius fails SAFE to the full panel: no grounder (or a
    // zero-grounding coverage query) leaves the size/high-risk-path signals with nothing
    // to assess, so routing LIGHT here would let a tiers-without-a-grounder workflow
    // silently downgrade EVERY unit's review - a fail-OPEN in a safety mechanism. Full is
    // the only defensible default when risk cannot be measured.
    let empty_radius = blast_radius.is_empty();
    let full_panel = empty_radius || high_risk_hit.is_some() || over_threshold || flapped;
    TierRouting {
        panel: if full_panel { full } else { &depth.light },
        tier: if full_panel { TIER_FULL } else { TIER_LIGHT },
        blast_radius: blast_radius.len(),
        threshold: depth.threshold,
        high_risk_hit,
        empty_radius,
        flapped,
        policy: true,
    }
}

/// An adjudicator's verdict gates the stage, FAIL-CLOSED: ONLY an explicit
/// `{"verdict":"approve"}` (the verdict field, case-insensitively "approve", on a
/// JSON line in the output) approves and lets integration proceed. Anything else -
/// no JSON, no `verdict` field, prose, `reject`, or any unrecognized value - does
/// NOT approve and routes the unit to remediation. A missing or unparseable verdict
/// is treated as a non-approval, never a silent pass.
///
/// `pub(crate)` so the canary runner (spec 13, unit 5) judges its adjudicator's verdict
/// through the SAME single fail-closed authority the live review path uses - the canary
/// measures the real gate, not a second parallel verdict parser that could disagree.
pub fn verdict_approves(output: &str) -> bool {
    last_verdict(output).is_some_and(|v| v.eq_ignore_ascii_case(VERDICT_APPROVE))
}

/// The verdict value (case-insensitive) that APPROVES a unit on the result channel: the
/// single literal [`verdict_approves`] recognizes. Exposed as a `pub const` so any facing
/// surface that must name the verdict line - the generated `using-rigger` discipline (spec
/// 20) - reads the SAME definition the gate reads, instead of hand-copying the word and
/// letting it silently drift from the gate.
pub const VERDICT_APPROVE: &str = "approve";

/// The VERDICT LINE of `output`: its LAST JSON line that carries a top-level `verdict`
/// string field, or `None` when `output` has none (no JSON, or JSON without a `verdict`
/// string). This is the SINGLE place a verdict line is recognized on the result channel:
/// [`verdict_approves`] reads its value for the fail-closed approval, the runtime
/// verdict-channel-mismatch backstop (spec 18, unit 3, [`has_verdict_line`]) reads its
/// PRESENCE to tell a gating spawn that DECIDED a verdict on the result channel (approve or
/// reject) from one that returned none at all, and [`verdict_required`] reads the items a
/// reject requires fixed.
fn verdict_line(output: &str) -> Option<Value> {
    output.lines().rev().find_map(|line| {
        serde_json::from_str::<Value>(line.trim())
            .ok()
            .filter(|v| v.get("verdict").is_some_and(Value::is_string))
    })
}

/// The verdict value on `output`'s [`verdict_line`], or `None` when it has none.
fn last_verdict(output: &str) -> Option<String> {
    verdict_line(output)?
        .get("verdict")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// One item a reject's verdict line requires fixed (its `required` list): what must change,
/// the repo-relative file it is in, whether it is a correctness defect - the adjudicator's
/// judgment, which keeps an item outside a later review round's delta blocking - and, for a
/// defect that recurs across sites, its shape (empty for a single site).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RequiredItem {
    pub finding: String,
    pub path: String,
    pub correctness: bool,
    pub pattern: String,
}

/// The items `output`'s [`verdict_line`] requires fixed: empty when there is no verdict line,
/// it carries no `required` list, or the list holds anything but items.
pub fn verdict_required(output: &str) -> Vec<RequiredItem> {
    verdict_line(output)
        .and_then(|v| serde_json::from_value(v.get("required")?.clone()).ok())
        .unwrap_or_default()
}

/// Whether `output` carries ANY parseable verdict line - a JSON line with a top-level
/// `verdict` string field - regardless of its value (spec 18, unit 3). A gating spawn
/// with one DECIDED (approve or reject) on the result channel the integration gate reads;
/// a gating spawn with none returned NO verdict at all, the trigger the runtime
/// verdict-channel-mismatch backstop pairs with an emit-only approve.
pub fn has_verdict_line(output: &str) -> bool {
    last_verdict(output).is_some()
}

/// Whether an event VALUE emitted via `rigger_emit` carries an approve-shaped verdict
/// (spec 18, unit 3). Serializes the value to its one-line JSON and runs it through
/// [`verdict_approves`], so the SINGLE approve-recognition authority judges an emitted
/// verdict exactly as it judges a result-channel one - the runtime backstop cannot drift
/// from the gate it protects.
pub fn emitted_verdict_approves(v: &Value) -> bool {
    verdict_approves(&serde_json::to_string(v).unwrap_or_default())
}

/// The ALREADY-INTEGRATED unit an adjudicator's verdict names as the COMPENSATION target
/// (spec 12, unit 4): the `compensate` field (a unit id) on the last JSON verdict line that
/// carries a non-empty one, or `None`. A later unit's review that proves a PRIOR integrated
/// unit wrong names it here; the conductor then reverts that unit's integrating commit(s)
/// and re-enters it into remediation. Parsed INDEPENDENTLY of the approve/reject verdict -
/// a review may approve the CURRENT unit's own work yet still name an integrated unit as the
/// real defect source. A missing or unparseable field is simply no compensation, never a
/// silent one.
pub fn verdict_compensates(output: &str) -> Option<String> {
    for line in output.lines().rev() {
        if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
            if let Some(target) = v.get("compensate").and_then(|x| x.as_str()) {
                let target = target.trim();
                if !target.is_empty() {
                    return Some(target.to_string());
                }
            }
        }
    }
    None
}

/// The adversary's routed review roster (spec 67, criterion 4): the unit's lens AGENT ids
/// (`ReviewPanel::lenses` / [`fan_out_lenses`]'s raw values), rendered as the same
/// review-attribution role tokens ([`lens_role`]) a `ReviewFinding.by` already carries - so
/// the driver names EXACTLY who the adversary is grounding against, never a guessed set.
/// Pure over the panel's own lens list, so it stays correct for whichever panel the
/// conductor actually routed to (light or full) - the caller always passes THAT panel's
/// lenses, never a static declaration.
pub fn review_roster(lenses: &[String]) -> Vec<String> {
    lenses.iter().map(|id| lens_role(id)).collect()
}

/// The adjudicator's routed review roster (spec 67, criterion 4): [`review_roster`]'s same
/// lens roster, PLUS [`ROLE_ADVERSARY`] when an adversary tier actually ran for this panel
/// (`adversary_id` non-empty) - never a fabricated entry for a panel with no adversary
/// (e.g. a reduced light tier).
pub fn adjudicator_roster(lenses: &[String], adversary_id: &str) -> Vec<String> {
    let mut roster = review_roster(lenses);
    if !adversary_id.is_empty() {
        roster.push(ROLE_ADVERSARY.to_string());
    }
    roster
}
