//! The review-verdict logic: the fail-closed verdict-line reading on the result channel, the
//! risk-tiered review-depth routing, the review rosters, and the spec critique (spec 112): its
//! prompt, its finding lines and the record keyed on a spec's content hash.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::contextgraph::TYPE_DECISION_MADE;
use crate::eventstore::{Error as StoreError, Event, EventStore, Position, TypeSelection};
use crate::playbooks::fnv1a_64;
use crate::spawn::{
    attempt_of, lens_role, requests, result_of, spawn_id, SpawnEvent, SpawnResult, ROLE_ADVERSARY,
    TYPE_SPAWN_REQUESTED, TYPE_SPAWN_RESULT,
};

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

/// The verdict value on the LAST JSON line of `output` that carries a top-level
/// `verdict` string field, or `None` when `output` has NO parseable verdict line (no
/// JSON, or JSON without a `verdict` string). This is the SINGLE place a verdict line is
/// recognized on the result channel: [`verdict_approves`] reads its VALUE for the
/// fail-closed approval, and the runtime verdict-channel-mismatch backstop (spec 18,
/// unit 3, [`has_verdict_line`]) reads its PRESENCE to tell a gating spawn that DECIDED a
/// verdict on the result channel (approve or reject) from one that returned none at all.
fn last_verdict(output: &str) -> Option<String> {
    for line in output.lines().rev() {
        if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
            if let Some(verdict) = v.get("verdict").and_then(|x| x.as_str()) {
                return Some(verdict.to_string());
            }
        }
    }
    None
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

/// The plan-critique rules both critique prompts read (spec 112): the Rule 7 and Rule 8 bullets
/// and the note on shared blast radius, exactly as the DAG critique has always pushed them. The
/// DAG critique pushes them after its opener; the spec critique pushes them after its stance,
/// which tells the critic a unit reads as a criterion and a reject as a BLOCKING finding.
pub const PLAN_CRITIQUE_RULES: &str =
    "- Rule 7 (mitigation ownership): every demanded mitigation must be owned by \
     exactly one unit, with the exclusion named on its neighbors; an unassigned or \
     ambiguously-owned mitigation - two units that will fight over the same concern \
     through the shared context graph - is a reject.\n\
     - Rule 8 (open dispositions): a unit must not leave a disposition open for a \
     reviewer to re-litigate; an undecided disposition is a reject.\n\
     NOTE on shared blast radius: units whose file footprints OVERLAP are NOT a \
     defect. `partition: by-blast-radius` runs them in SEPARATE sequential batches \
     (each branches off the prior batch's integrated tree), and per-unit worktree \
     isolation keeps every reviewer on its own diff - so overlap integrates cleanly \
     and reviews independently. Do NOT reject merely because two units touch the \
     same file. Reject a shared-file split ONLY when it is a genuine OWNERSHIP or \
     COHERENCE defect (rule 7) - two units that cannot own their concern cleanly - \
     not for mechanical overlap the partitioner already serializes.\n\n";

/// Section (i) of the spec critique prompt: the stance, and the persona and discipline duties a
/// spec critique voids.
const CRITIQUE_STANCE: &str = "This is a critique of a SPEC, not code: the text below is a design \
     and its Done-when criteria, written before any run builds them. Default to skepticism: \
     assume the author missed something, and try to prove the spec self-contradictory or \
     undecided; never soften a finding to converge. Cite the criterion number or Design block \
     title for every finding. In the rules below a unit reads as a criterion, and a reject as a \
     BLOCKING finding. Every persona or discipline duty that conflicts with a spec critique is \
     void for this task: reviewing lenses and a diff, the rule against rendering a verdict, \
     running gates or any build or test command, editing a file, and recording through \
     `rigger_emit`, `rigger_progress` or `rigger_scratch`. Read the spec and the repository; the \
     finding lines and the verdict line of your final message are the only output.\n\nThe rules:\n";

/// Section (iii): the two ownership defects a spec can carry.
const CRITIQUE_OWNERSHIP: &str = "Ownership defects in a spec: twin criteria (two checkboxes \
     claiming one concern) and bundling (one checkbox carrying two mitigations) are each a \
     BLOCKING finding.\n\n";

/// Section (iv): the two named hunts.
const CRITIQUE_HUNTS: &str = "Run two hunts, and name the hunt in each finding it produces:\n\
     - LANDING ORDER: for every ordered pair of criteria (A, B) sharing a command, a store read, \
     a file or a counter, ask \"if A lands first on a tree without B, does A's own text hold?\"; \
     every no is a finding.\n\
     - UNDECIDED CORNER: for every criterion's mechanism, walk the corners empty, repeated, \
     reverted, DROPPED (a fact present in an earlier generation and absent in a later one), \
     concurrent, crash-resume, cold start and existing data; each corner no Design sentence \
     decides is a finding.\n\n";

/// Section (v): the ban on criterion edits.
const CRITIQUE_BAN: &str = "The ban: a fix is a Design or Global-constraint change, never a \
     criterion edit.\n\n";

/// Section (vi): the critic's output contract.
const CRITIQUE_OUTPUT_CONTRACT: &str = "The output contract: one finding per line, then the \
     verdict as the last line:\n\n\
     <critic id> | BLOCKING | <criterion n or Design block title> | <exact reading that breaks> \
     | <smallest Design change that closes it>\n\
     <critic id> | NON-BLOCKING | ...\n\
     {\"verdict\":\"reject\"}\n\n\
     A finding line has five `|`-separated fields whose second is exactly BLOCKING or \
     NON-BLOCKING; any other line is prose. `<critic id>` is your own short label for the \
     finding. The verdict is reject when any line is BLOCKING, else approve.\n\n";

/// The spec critique prompt (spec 112): the stance, [`PLAN_CRITIQUE_RULES`], the ownership
/// defects, the two hunts, the ban on criterion edits, the output contract, and the spec text
/// verbatim as its tail - in that order, from the spec path and text alone.
pub fn spec_critique_prompt(spec_path: &str, spec_text: &str) -> String {
    format!(
        "{CRITIQUE_STANCE}{PLAN_CRITIQUE_RULES}{CRITIQUE_OWNERSHIP}{CRITIQUE_HUNTS}{CRITIQUE_BAN}\
         {CRITIQUE_OUTPUT_CONTRACT}The spec under critique, `{spec_path}`, verbatim:\n\n{spec_text}"
    )
}

/// The tools the critic runs with, in place of its persona's own: it reads and looks things up
/// in the graph, and can neither build nor record.
pub const CRITIC_TOOLS: [&str; 5] = [
    "Read",
    "Glob",
    "mcp__rigger__rigger_graph",
    "mcp__rigger__rigger_ground",
    "mcp__rigger__rigger_peers",
];

/// The tools the critic is denied, whatever the operator's own settings allow (a deny wins over
/// every allow source): every tool that builds, records or edits - the shell, the fan-out tools,
/// the search tool the critic reads through the graph instead, every editing tool, and the
/// recording tools of the spawn's own rigger server.
pub const CRITIC_DENIED_TOOLS: [&str; 11] = [
    "Bash",
    "Agent",
    "Task",
    "Grep",
    "Edit",
    "MultiEdit",
    "Write",
    "NotebookEdit",
    "mcp__rigger__rigger_emit",
    "mcp__rigger__rigger_progress",
    "mcp__rigger__rigger_scratch",
];

/// The `by` a critique finding's graph copy carries.
pub const SPEC_CRITIC: &str = "spec-critic";

/// The prefix of a critique run's id; the hash follows it.
const CRITIQUE_RUN_PREFIX: &str = "critique-";

/// A spec text's content hash: [`fnv1a_64`] over its raw bytes, as 16 lowercase hex digits. Any
/// byte change is new text. The only place a critique hash is rendered.
pub fn critique_hash(text: &str) -> String {
    format!("{:016x}", fnv1a_64(text.as_bytes()))
}

/// The critique run (and unit) of a hash: `critique-<hash>`.
pub fn critique_unit(hash: &str) -> String {
    format!("{CRITIQUE_RUN_PREFIX}{hash}")
}

/// The critic's spawn id for a hash at `attempt`: the adversary of the hash's critique run.
pub fn critique_spawn_id(hash: &str, attempt: u32) -> String {
    spawn_id(&critique_unit(hash), ROLE_ADVERSARY, attempt)
}

/// Whether a scratch directory name is a critique run's: `critique-` and a 16-lowercase-hex-digit
/// hash, the shape [`critique_unit`] renders.
pub fn is_critique_run(name: &str) -> bool {
    name.strip_prefix(CRITIQUE_RUN_PREFIX).is_some_and(|hash| {
        hash.len() == 16
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// The attempt a critique spawn id of `hash` carries, or `None` when `id` is not exactly
/// [`critique_spawn_id`] of the hash at some attempt: the ordinal the id grammar's own reader
/// ([`attempt_of`]) returns, kept only when minting it back gives `id` itself.
fn critique_attempt(id: &str, hash: &str) -> Option<u32> {
    let attempt = attempt_of(id);
    (critique_spawn_id(hash, attempt) == id).then_some(attempt)
}

/// One finding line of a critique, as the critic wrote it, with the id the record gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CritiqueFinding {
    /// `sc-<hash>-<attempt>-<k>`, `k` the 1-based order of the finding line.
    pub id: String,
    /// Whether the line's severity is `BLOCKING` (else `NON-BLOCKING`).
    pub blocking: bool,
    /// The critic's own label for the finding (the first field).
    pub critic: String,
    /// The criterion number or Design block title the finding cites.
    pub location: String,
    /// The exact reading that breaks.
    pub reading: String,
    /// The smallest Design change that closes it.
    pub fix: String,
}

impl CritiqueFinding {
    /// The finding without its critic label: `<severity> | <location> | <reading> | <fix>`.
    pub fn summary(&self) -> String {
        let severity = if self.blocking {
            "BLOCKING"
        } else {
            "NON-BLOCKING"
        };
        format!(
            "{severity} | {} | {} | {}",
            self.location, self.reading, self.fix
        )
    }
}

/// The finding lines of a critic's `output`, in order, with their record ids. A line is a finding
/// when, once trimmed and stripped of one leading and one trailing pipe, it splits on `|` into at
/// least five fields whose trimmed second is exactly `BLOCKING` or `NON-BLOCKING`; the fields past
/// the fourth are joined back with `|` into the fix. Every other line is prose.
pub fn critique_findings(output: &str, hash: &str, attempt: u32) -> Vec<CritiqueFinding> {
    let mut findings = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let line = line.strip_prefix('|').unwrap_or(line);
        let line = line.strip_suffix('|').unwrap_or(line);
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() < 5 {
            continue;
        }
        let blocking = match fields[1].trim() {
            "BLOCKING" => true,
            "NON-BLOCKING" => false,
            _ => continue,
        };
        findings.push(CritiqueFinding {
            id: format!("sc-{hash}-{attempt}-{}", findings.len() + 1),
            blocking,
            critic: fields[0].trim().to_string(),
            location: fields[2].trim().to_string(),
            reading: fields[3].trim().to_string(),
            fix: fields[4..].join("|").trim().to_string(),
        });
    }
    findings
}

/// A spec's critique: a recorded critic result that passed the authority ([`critique_of`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Critique {
    /// The global log position of its `SpawnResult`.
    pub position: Position,
    /// The attempt the critic ran at.
    pub attempt: u32,
    /// The verdict value as the critic wrote it.
    pub verdict: String,
    /// Its finding lines, in order.
    pub findings: Vec<CritiqueFinding>,
}

/// THE AUTHORITY over one recorded `SpawnResult` event: it is a critique of `hash` when its id is
/// the hash's critic spawn at some attempt, its error is empty, its output carries a verdict line,
/// and a verdict that does not approve comes with at least one BLOCKING finding. `Err` says why
/// it is not one.
pub fn critique_of(event: &Event, hash: &str) -> Result<Critique, String> {
    let result = SpawnResult::from_event(event)
        .map_err(|e| format!("the recorded result is unreadable: {e}"))?;
    let (attempt, verdict, findings) = judged(&result, hash)?;
    Ok(Critique {
        position: event.position,
        attempt,
        verdict,
        findings,
    })
}

/// The authority's judgment of one decoded result: its attempt, verdict and findings when it is a
/// critique of `hash`, else why it is not one.
fn judged(result: &SpawnResult, hash: &str) -> Result<(u32, String, Vec<CritiqueFinding>), String> {
    let attempt = critique_attempt(&result.id, hash)
        .ok_or_else(|| format!("{} is not a critique spawn of {hash}", result.id))?;
    if !result.error.is_empty() {
        return Err(format!("the critic's spawn failed: {}", result.error));
    }
    let verdict = last_verdict(&result.output)
        .ok_or_else(|| "the critic's output carries no verdict line".to_string())?;
    let findings = critique_findings(&result.output, hash, attempt);
    if !verdict_approves(&result.output) && !findings.iter().any(|f| f.blocking) {
        return Err(format!(
            "the critic's verdict {verdict:?} does not approve, yet its output carries no \
             BLOCKING finding line"
        ));
    }
    Ok((attempt, verdict, findings))
}

/// The critique of `hash` among `events`: its latest critique (highest position), or `None`.
pub fn latest_critique(events: &[Event], hash: &str) -> Option<Critique> {
    events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .filter_map(|e| critique_of(e, hash).ok())
        .max_by_key(|c| c.position)
}

/// The `SpawnResult` events of `store`'s run stream (the critique's own store): one typed read,
/// materializing no other event.
fn critique_results(store: &dyn EventStore) -> Result<Vec<Event>, StoreError> {
    store.read_stream_typed(
        crate::run::STREAM,
        0,
        TypeSelection::Only(&[TYPE_SPAWN_RESULT]),
    )
}

/// Read the critique of `hash` from `store` (the critique's own store): one typed read of the
/// `SpawnResult` events of its run stream, materializing no other event.
pub fn read_critique(store: &dyn EventStore, hash: &str) -> Result<Option<Critique>, StoreError> {
    Ok(latest_critique(&critique_results(store)?, hash))
}

/// What `store` (the critique's own store) answers for `hash` once the critic's spawn at
/// `attempt` returned, from one typed read: the hash's critique ([`latest_critique`]) when one is
/// recorded, else why there is none - the spawn's own latest result read by the authority, or
/// `None` when the spawn recorded no result.
pub fn read_spawned_critique(
    store: &dyn EventStore,
    hash: &str,
    attempt: u32,
) -> Result<Result<Critique, Option<String>>, StoreError> {
    let results = critique_results(store)?;
    Ok(latest_critique(&results, hash).ok_or_else(|| why_no_critique(&results, hash, attempt)))
}

/// Why the critic's spawn of `hash` at `attempt` is no critique: its latest result among
/// `results` ([`result_of`]) judged by the authority, `None` when it recorded none. A result on
/// the stream that cannot be read is the why, since the spawn's own result cannot be told apart.
fn why_no_critique(results: &[Event], hash: &str, attempt: u32) -> Option<String> {
    match result_of(results, &critique_spawn_id(hash, attempt)) {
        Ok(result) => result.and_then(|result| judged(&result, hash).err()),
        Err(e) => Some(format!(
            "a result recorded on the critique stream is unreadable: {e}"
        )),
    }
}

/// The attempt the next critic spawn of `hash` runs at: how many `SpawnRequested` events `store`
/// (the critique's own store) already records for the hash, read by type. A request with no
/// result, or with a result that is no critique, still counts, so the next call runs the next
/// attempt; a request that cannot be read fails the count, so an attempt id is never reused.
pub fn next_critique_attempt(store: &dyn EventStore, hash: &str) -> Result<u32, StoreError> {
    let events = store.read_stream_typed(
        crate::run::STREAM,
        0,
        TypeSelection::Only(&[TYPE_SPAWN_REQUESTED]),
    )?;
    critique_requests(&events, hash).map_err(|e| {
        StoreError::Backend(format!(
            "a spawn request recorded on the critique stream is unreadable: {e}"
        ))
    })
}

/// How many of the spawn requests among `events` ([`requests`], one per `SpawnRequested` event,
/// so a re-parked id counts once per park) request a critic spawn of `hash`.
fn critique_requests(events: &[Event], hash: &str) -> Result<u32, serde_json::Error> {
    let mut requested = 0;
    for request in requests(events)? {
        if critique_attempt(&request.id, hash).is_some() {
            requested += 1;
        }
    }
    Ok(requested)
}

/// The `ReviewFinding` payload that copies a critique finding into the graph, about the spec.
pub fn finding_copy(finding: &CritiqueFinding, spec: &str) -> Value {
    serde_json::json!({
        "id": finding.id,
        "by": SPEC_CRITIC,
        "summary": finding.summary(),
        "about": [spec],
    })
}

/// The root a spec path is made relative to: the repository (`repo`, the main worktree's root)
/// when there is one, else `cwd`, the project root holding `.rigger/`.
pub fn spec_root(cwd: &Path, repo: &str) -> PathBuf {
    if repo.is_empty() {
        cwd.to_path_buf()
    } else {
        PathBuf::from(repo)
    }
}

/// `path`'s components resolved lexically: `.` dropped, `..` popping its parent, `None` when a
/// `..` climbs above the start. No filesystem access.
fn lexical_parts(path: &Path) -> Option<Vec<String>> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    Some(parts)
}

/// A spec path made relative to `root`, lexically (no filesystem access, so a symlinked absolute
/// spelling reads as outside): `.` components dropped, `..` resolved, an absolute path stripped
/// of `root` component by component, the rest joined with `/`. `None` is the outside verdict: an
/// absolute path not under `root`, or a `..` that climbs above it.
pub fn normalize_spec_path(root: &Path, path: &str) -> Option<String> {
    let path = Path::new(path);
    let parts = lexical_parts(path)?;
    let relative = if path.is_absolute() {
        parts
            .strip_prefix(lexical_parts(root)?.as_slice())?
            .to_vec()
    } else {
        parts
    };
    Some(relative.join("/"))
}

/// The string entries of a JSON array, none when `value` is no array.
fn string_entries(value: &Value) -> impl Iterator<Item = &str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// The open BLOCKING findings of `critique` (spec 112, A NEW RUN IS REFUSED UNTIL ITS CRITIQUE IS
/// CLEAN): the ids of its BLOCKING findings, in finding order, minus every id named in the
/// `resolves` of a `DecisionMade` among `decisions` that the store recorded after the critique's
/// result (a greater position) and whose `governs` names `spec` - the spec path already made
/// repo-relative - once each entry is normalized against `root` like the spec path
/// ([`normalize_spec_path`]). An id the critique does not hold is ignored, and supersession is not
/// read: a recorded resolution stands until the text changes.
pub fn open_findings(
    critique: &Critique,
    decisions: &[Event],
    spec: &str,
    root: &Path,
) -> Vec<String> {
    let resolved: Vec<String> = decisions
        .iter()
        .filter(|event| event.type_ == TYPE_DECISION_MADE && event.position > critique.position)
        .filter_map(Event::decode::<Value>)
        .filter(|decision| {
            string_entries(&decision["governs"])
                .any(|governed| normalize_spec_path(root, governed).as_deref() == Some(spec))
        })
        .flat_map(|decision| {
            string_entries(&decision["resolves"])
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    critique
        .findings
        .iter()
        .filter(|finding| finding.blocking && !resolved.contains(&finding.id))
        .map(|finding| finding.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_fixtures::ev_at;
    use crate::spawn::TYPE_SPAWN_RESULT;
    use serde_json::json;
    use std::path::{Path, PathBuf};

    const HASH: &str = "0123456789abcdef";

    /// A recorded `SpawnResult` for `id` at log position `position`.
    fn result_at(position: u64, id: &str, output: &str, error: &str) -> Event {
        ev_at(
            position,
            TYPE_SPAWN_RESULT,
            json!({"id": id, "output": output, "error": error}),
        )
    }

    /// The finding a test expects, its fields in finding-line order.
    fn finding(id: &str, blocking: bool, fields: [&str; 4]) -> CritiqueFinding {
        let [critic, location, reading, fix] = fields;
        CritiqueFinding {
            id: id.to_string(),
            blocking,
            critic: critic.to_string(),
            location: location.to_string(),
            reading: reading.to_string(),
            fix: fix.to_string(),
        }
    }

    #[test]
    fn the_critique_hash_is_fnv1a_64_of_the_raw_bytes_as_sixteen_lowercase_hex_digits() {
        assert_eq!(critique_hash(""), "cbf29ce484222325");
        assert_eq!(critique_hash("a"), "af63dc4c8601ec8c");
        assert_ne!(
            critique_hash("a\n"),
            critique_hash("a"),
            "a whitespace-only edit is new text"
        );
    }

    #[test]
    fn a_critique_spawn_is_the_adversary_of_the_hash_critique_run() {
        assert_eq!(critique_unit(HASH), "critique-0123456789abcdef");
        assert_eq!(
            critique_spawn_id(HASH, 2),
            "critique-0123456789abcdef/adversary#2"
        );
    }

    #[test]
    fn a_critique_run_directory_is_critique_and_sixteen_lowercase_hex_digits() {
        assert!(is_critique_run("critique-0123456789abcdef"));
        for other in [
            "critique-0123456789abcde",
            "critique-0123456789abcdef0",
            "critique-0123456789ABCDEF",
            "critique-0123456789abcdeg",
            "run-0123456789abcdef",
            "0123456789abcdef",
            "critique-",
        ] {
            assert!(!is_critique_run(other), "{other:?} is not a critique run");
        }
    }

    #[test]
    fn a_finding_line_has_five_pipe_fields_with_the_severity_second() {
        let output = "I read the spec.\n\
            C1 | BLOCKING | criterion 2 | reads A | decide A in Design\n\
            \x20 | C2 | NON-BLOCKING | THE STORE | reads B | add a sentence |  \n\
            C3 | BLOCKING | criterion 1 | the fix has a pipe | use `a | b` | here\n\
            C4 | blocking | criterion 3 | lowercase severity | is prose\n\
            C5 | BLOCKING | criterion 4 | only four fields\n\
            | id | severity | where | reading | fix |\n\
            |---|---|---|---|---|\n\
            {\"verdict\":\"reject\"}\n";
        assert_eq!(
            critique_findings(output, HASH, 3),
            vec![
                finding(
                    "sc-0123456789abcdef-3-1",
                    true,
                    ["C1", "criterion 2", "reads A", "decide A in Design"]
                ),
                finding(
                    "sc-0123456789abcdef-3-2",
                    false,
                    ["C2", "THE STORE", "reads B", "add a sentence"]
                ),
                finding(
                    "sc-0123456789abcdef-3-3",
                    true,
                    [
                        "C3",
                        "criterion 1",
                        "the fix has a pipe",
                        "use `a | b` | here"
                    ]
                ),
            ],
            "the fields past the fourth join back into the fix; every other line is prose"
        );
        assert_eq!(critique_findings("no findings at all", HASH, 0), vec![]);
    }

    #[test]
    fn a_finding_summary_is_its_severity_and_its_last_three_fields() {
        let blocking = finding("sc-x-0-1", true, ["C1", "criterion 2", "reads A", "fix A"]);
        assert_eq!(
            blocking.summary(),
            "BLOCKING | criterion 2 | reads A | fix A"
        );
        let advisory = finding("sc-x-0-2", false, ["C2", "Design", "reads B", "fix B"]);
        assert_eq!(
            advisory.summary(),
            "NON-BLOCKING | Design | reads B | fix B"
        );
    }

    #[test]
    fn a_finding_copy_is_a_spec_critic_review_finding_about_the_spec() {
        let f = finding(
            "sc-0123456789abcdef-0-1",
            true,
            ["C1", "criterion 2", "reads A", "fix A"],
        );
        assert_eq!(
            finding_copy(&f, "specs/9-demo.md"),
            json!({
                "id": "sc-0123456789abcdef-0-1",
                "by": "spec-critic",
                "summary": "BLOCKING | criterion 2 | reads A | fix A",
                "about": ["specs/9-demo.md"],
            })
        );
    }

    #[test]
    fn a_result_is_a_critique_only_with_no_error_a_verdict_and_a_blocking_finding_behind_a_reject()
    {
        let id = critique_spawn_id(HASH, 0);
        let why = |output: &str, error: &str| {
            critique_of(&result_at(1, &id, output, error), HASH).unwrap_err()
        };
        assert_eq!(
            why("{\"verdict\":\"approve\"}", "boom"),
            "the critic's spawn failed: boom"
        );
        assert_eq!(
            why("C1 | BLOCKING | c | r | f", ""),
            "the critic's output carries no verdict line"
        );
        assert_eq!(
            why(
                "C1 | NON-BLOCKING | c | r | f\n{\"verdict\":\"reject\"}",
                ""
            ),
            "the critic's verdict \"reject\" does not approve, yet its output carries no \
             BLOCKING finding line"
        );
        assert_eq!(
            critique_of(
                &result_at(1, "plan/adversary#0", "{\"verdict\":\"approve\"}", ""),
                HASH
            )
            .unwrap_err(),
            "plan/adversary#0 is not a critique spawn of 0123456789abcdef"
        );
        let unreadable = ev_at(1, TYPE_SPAWN_RESULT, json!({"output": "no id"}));
        assert!(
            critique_of(&unreadable, HASH)
                .unwrap_err()
                .starts_with("the recorded result is unreadable: "),
            "a result body with no id is no critique"
        );
    }

    #[test]
    fn an_approve_beside_a_blocking_line_is_a_critique_whose_finding_still_blocks() {
        let id = critique_spawn_id(HASH, 4);
        let output =
            "C1 | BLOCKING | c | r | f\n{\"verdict\":\"approve\"}\nprose after the verdict";
        assert_eq!(
            critique_of(&result_at(7, &id, output, ""), HASH),
            Ok(Critique {
                position: 7,
                attempt: 4,
                verdict: "approve".to_string(),
                findings: vec![finding(
                    "sc-0123456789abcdef-4-1",
                    true,
                    ["C1", "c", "r", "f"]
                )],
            })
        );
        assert_eq!(
            critique_of(
                &result_at(3, &id, "No defects.\n{\"verdict\":\"approve\"}", ""),
                HASH
            ),
            Ok(Critique {
                position: 3,
                attempt: 4,
                verdict: "approve".to_string(),
                findings: vec![],
            }),
            "an approving verdict and no finding line is a clean critique"
        );
    }

    #[test]
    fn the_critique_of_a_hash_is_its_highest_position_critique() {
        let reject = "C1 | BLOCKING | criterion 1 | r | f\n{\"verdict\":\"reject\"}";
        let approve = "{\"verdict\":\"approve\"}";
        let events = vec![
            result_at(5, &critique_spawn_id(HASH, 0), reject, ""),
            result_at(9, &critique_spawn_id(HASH, 1), approve, ""),
            result_at(12, &critique_spawn_id("fedcba9876543210", 2), approve, ""),
            result_at(14, &format!("critique-{HASH}/implementer#3"), approve, ""),
            result_at(15, &format!("critique-{HASH}/adversary#04"), approve, ""),
            result_at(16, &critique_spawn_id(HASH, 5), "", "the spawn failed"),
            ev_at(17, "DecisionMade", json!({"id": "d", "summary": "s"})),
        ];
        assert_eq!(
            latest_critique(&events, HASH),
            Some(Critique {
                position: 9,
                attempt: 1,
                verdict: "approve".to_string(),
                findings: vec![],
            })
        );
        assert_eq!(
            latest_critique(&events[..1], HASH),
            Some(Critique {
                position: 5,
                attempt: 0,
                verdict: "reject".to_string(),
                findings: vec![finding(
                    "sc-0123456789abcdef-0-1",
                    true,
                    ["C1", "criterion 1", "r", "f"]
                )],
            })
        );
        assert_eq!(
            latest_critique(&events[2..], HASH),
            None,
            "another hash, another role, a non-canonical attempt, an error and a non-result \
             are no critique of the hash"
        );
    }

    /// A recorded `SpawnRequested` for `id` at log position `position`.
    fn request_at(position: u64, id: &str) -> Event {
        ev_at(
            position,
            crate::spawn::TYPE_SPAWN_REQUESTED,
            json!({"id": id, "unit": "u", "stage": "critique", "prompt": "p"}),
        )
    }

    #[test]
    fn the_next_attempt_counts_the_requests_whose_id_is_a_critic_spawn_of_the_hash() {
        let events = vec![
            request_at(1, &critique_spawn_id(HASH, 0)),
            ev_at(2, "DecisionMade", json!({"id": "d", "summary": "s"})),
            result_at(3, &critique_spawn_id(HASH, 0), "", "the spawn failed"),
            request_at(4, &critique_spawn_id(HASH, 1)),
            request_at(5, &critique_spawn_id("fedcba9876543210", 0)),
            request_at(6, &format!("critique-{HASH}/implementer#2")),
            request_at(7, &format!("critique-{HASH}/adversary#03")),
        ];
        assert_eq!(
            critique_requests(&events, HASH).unwrap(),
            2,
            "a request with no result or a failed one still counts; another hash, another role, \
             a non-canonical attempt and a non-request do not"
        );
        assert_eq!(critique_requests(&events[1..3], HASH).unwrap(), 0);
        let unreadable = ev_at(
            8,
            crate::spawn::TYPE_SPAWN_REQUESTED,
            json!({"id": critique_spawn_id(HASH, 2)}),
        );
        assert!(
            critique_requests(&[unreadable], HASH)
                .unwrap_err()
                .to_string()
                .starts_with("missing field `unit`"),
            "an unreadable request fails the count, so an attempt id is never reused"
        );
    }

    #[test]
    fn two_requests_parked_under_one_critic_spawn_id_count_twice() {
        // Design spec 112: the attempt is the number of `SpawnRequested` events recorded for the
        // hash. Two calls racing on one text can mint one id and park it twice; the next attempt
        // is then 2, never the 1 a count of distinct ids would give.
        let events = vec![
            request_at(1, &critique_spawn_id(HASH, 0)),
            request_at(2, &critique_spawn_id(HASH, 0)),
        ];
        assert_eq!(critique_requests(&events, HASH).unwrap(), 2);
    }

    #[test]
    fn the_why_of_a_spawn_with_no_critique_is_its_own_latest_result_read_by_the_authority() {
        let attempt_1 = critique_spawn_id(HASH, 1);
        let reject = "C1 | BLOCKING | criterion 1 | r | f\n{\"verdict\":\"reject\"}";
        let events = vec![
            result_at(1, &critique_spawn_id(HASH, 0), reject, ""),
            result_at(2, &attempt_1, "", "boom"),
            result_at(3, &attempt_1, "I could not decide.", ""),
            ev_at(4, "DecisionMade", json!({"id": "d", "summary": "s"})),
        ];
        assert_eq!(
            why_no_critique(&events, HASH, 1).as_deref(),
            Some("the critic's output carries no verdict line"),
            "the latest result of the spawn is the one judged"
        );
        assert_eq!(
            why_no_critique(&events[..3], HASH, 1).as_deref(),
            Some("the critic's output carries no verdict line")
        );
        assert_eq!(
            why_no_critique(&events[..2], HASH, 1).as_deref(),
            Some("the critic's spawn failed: boom")
        );
        assert_eq!(
            why_no_critique(&events, HASH, 2),
            None,
            "a spawn that recorded no result has no why of its own"
        );
        let unreadable = ev_at(5, TYPE_SPAWN_RESULT, json!({"output": "no id"}));
        let why = why_no_critique(&[events[2].clone(), unreadable], HASH, 1).unwrap();
        assert!(
            why.starts_with(
                "a result recorded on the critique stream is unreadable: missing field `id`"
            ),
            "an unreadable result on the stream is the why: {why}"
        );
    }

    #[test]
    fn the_plan_critique_rules_are_the_rule_seven_and_eight_bullets_and_the_overlap_note() {
        assert!(PLAN_CRITIQUE_RULES.starts_with("- Rule 7 (mitigation ownership): "));
        let rule_8 = PLAN_CRITIQUE_RULES
            .find("\n- Rule 8 (open dispositions): ")
            .expect("the Rule 8 bullet");
        let note = PLAN_CRITIQUE_RULES
            .find("\nNOTE on shared blast radius: ")
            .expect("the shared blast radius note");
        assert!(rule_8 < note, "Rule 8 precedes the note");
        assert!(PLAN_CRITIQUE_RULES.ends_with("the partitioner already serializes.\n\n"));
    }

    #[test]
    fn the_spec_critique_prompt_runs_its_sections_in_order_and_ends_with_the_spec_verbatim() {
        let text = "# 9 - A spec\n\n## Done when\n\n- [ ] a test proves X\n";
        let prompt = spec_critique_prompt("specs/9-a.md", text);
        let at = |needle: &str| {
            prompt
                .find(needle)
                .unwrap_or_else(|| panic!("the prompt carries {needle:?}:\n{prompt}"))
        };
        let order = [
            at(CRITIQUE_STANCE),
            at(PLAN_CRITIQUE_RULES),
            at(CRITIQUE_OWNERSHIP),
            at(CRITIQUE_HUNTS),
            at(CRITIQUE_BAN),
            at(CRITIQUE_OUTPUT_CONTRACT),
            at("The spec under critique, `specs/9-a.md`, verbatim:\n\n"),
        ];
        assert_eq!(order[0], 0, "the stance opens the prompt");
        assert!(
            order.windows(2).all(|w| w[0] < w[1]),
            "sections (i) to (vii) appear in that order: {order:?}"
        );
        assert!(prompt.ends_with(text), "the spec text is the verbatim tail");
        assert!(
            !prompt.contains("Unit size"),
            "no unit-size line in a spec critique"
        );
    }

    #[test]
    fn the_spec_critique_sections_carry_their_decided_duties() {
        for duty in [
            "critique of a SPEC, not code",
            "skepticism",
            "assume the author missed something",
            "self-contradictory or undecided",
            "never soften",
            "criterion number or Design block title",
            "a unit reads as a criterion",
            "a reject as a BLOCKING finding",
            "reviewing lenses and a diff",
            "the rule against rendering a verdict",
            "running gates or any build or test command",
            "editing a file",
            "`rigger_emit`, `rigger_progress` or `rigger_scratch`",
            "the finding lines and the verdict line of your final message are the only output",
        ] {
            assert!(CRITIQUE_STANCE.contains(duty), "the stance names {duty:?}");
        }
        for defect in [
            "twin criteria",
            "two checkboxes claiming one concern",
            "bundling",
        ] {
            assert!(
                CRITIQUE_OWNERSHIP.contains(defect),
                "ownership names {defect:?}"
            );
        }
        for hunt in [
            "LANDING ORDER",
            "if A lands first on a tree without B, does A's own text hold?",
            "UNDECIDED CORNER",
            "empty, repeated, reverted, DROPPED",
            "a fact present in an earlier generation and absent in a later one",
            "concurrent, crash-resume, cold start and existing data",
        ] {
            assert!(CRITIQUE_HUNTS.contains(hunt), "the hunts name {hunt:?}");
        }
        assert!(CRITIQUE_BAN
            .contains("a fix is a Design or Global-constraint change, never a criterion edit"));
        for line in [
            "<critic id> | BLOCKING | <criterion n or Design block title> | <exact reading that \
             breaks> | <smallest Design change that closes it>\n",
            "<critic id> | NON-BLOCKING | ...\n",
            "{\"verdict\":\"reject\"}\n",
        ] {
            assert!(
                CRITIQUE_OUTPUT_CONTRACT.contains(line),
                "the output contract carries {line:?}"
            );
        }
    }

    #[test]
    fn a_spec_path_is_made_repo_relative_lexically_and_refused_outside_the_root() {
        let root = Path::new("/work/repo");
        let norm = |p: &str| normalize_spec_path(root, p);
        assert_eq!(norm("specs/a.md").as_deref(), Some("specs/a.md"));
        assert_eq!(norm("./specs/./a.md").as_deref(), Some("specs/a.md"));
        assert_eq!(norm("/work/repo/specs/a.md").as_deref(), Some("specs/a.md"));
        assert_eq!(
            norm("/work/repo/specs/../docs/a.md").as_deref(),
            Some("docs/a.md")
        );
        assert_eq!(norm("specs/../a.md").as_deref(), Some("a.md"));
        for outside in [
            "/work/other/a.md",
            "/work/repository/a.md",
            "/work/repo/../a.md",
            "../repo/specs/a.md",
            "specs/../../a.md",
            "/../../a.md",
        ] {
            assert_eq!(norm(outside), None, "{outside:?} is outside {root:?}");
        }
    }

    /// A critique of [`HASH`] whose result sits at log position 10, holding `findings` as
    /// `(id, blocking)` in finding order.
    fn critique_at_ten(findings: &[(&str, bool)]) -> Critique {
        Critique {
            position: 10,
            attempt: 0,
            verdict: "reject".to_string(),
            findings: findings
                .iter()
                .map(|(id, blocking)| finding(id, *blocking, ["C", "criterion 1", "r", "f"]))
                .collect(),
        }
    }

    /// A `DecisionMade` at log position `position` governing `governs` and resolving `resolves`.
    fn resolution(position: u64, governs: &[&str], resolves: &[&str]) -> Event {
        ev_at(
            position,
            crate::contextgraph::TYPE_DECISION_MADE,
            json!({"id": format!("res-{position}"), "summary": "closed", "governs": governs,
                   "resolves": resolves}),
        )
    }

    /// Spec 112, A NEW RUN IS REFUSED UNTIL ITS CRITIQUE IS CLEAN: the open findings are the
    /// critique's BLOCKING ids, in finding order, minus each id a `DecisionMade` recorded after
    /// the critique's result names in `resolves` while its `governs` names the spec under any
    /// spelling that normalizes to it; nothing else closes one.
    #[test]
    fn the_open_findings_are_the_blocking_ids_no_later_resolution_of_the_spec_names() {
        let root = Path::new("/work/repo");
        let spec = "specs/a.md";
        let critique = critique_at_ten(&[
            ("b1", true),
            ("n2", false),
            ("b3", true),
            ("b4", true),
            ("b5", true),
        ]);
        let open = |decisions: &[Event]| open_findings(&critique, decisions, spec, root);
        assert_eq!(
            open(&[]),
            ["b1", "b3", "b4", "b5"],
            "unresolved: every BLOCKING id in finding order, never a NON-BLOCKING one"
        );
        assert_eq!(
            open(&[
                resolution(11, &["./specs/a.md"], &["b1"]),
                resolution(
                    12,
                    &["specs/other.md", "/work/repo/specs/a.md"],
                    &["b3", "n2", "ghost"]
                ),
            ]),
            ["b4", "b5"],
            "a later resolution governing the spec, relative or absolute, closes the ids it \
             names; a NON-BLOCKING or unknown id it names changes nothing"
        );
        assert_eq!(
            open(&[
                resolution(10, &[spec], &["b1"]),
                resolution(9, &[spec], &["b3"]),
                resolution(13, &["specs/other.md"], &["b4"]),
                resolution(14, &["/elsewhere/specs/a.md", "../specs/a.md"], &["b4"]),
                ev_at(
                    15,
                    crate::contextgraph::TYPE_LESSON_LEARNED,
                    json!({"id": "l", "governs": [spec], "resolves": ["b5"]})
                ),
                ev_at(
                    16,
                    crate::contextgraph::TYPE_DECISION_MADE,
                    json!({"id": "s", "governs": spec, "resolves": ["b5"]})
                ),
            ]),
            ["b1", "b3", "b4", "b5"],
            "a resolution at or before the critique's result, one governing another spec or a \
             path outside the root, another event type and a governs that is no list close nothing"
        );
        assert_eq!(
            open_findings(&critique_at_ten(&[("n1", false)]), &[], spec, root),
            Vec::<String>::new(),
            "a critique with no BLOCKING finding has none open"
        );
    }

    #[test]
    fn the_spec_root_is_the_repository_else_the_project_directory() {
        assert_eq!(
            spec_root(Path::new("/proj"), "/work/repo"),
            PathBuf::from("/work/repo")
        );
        assert_eq!(spec_root(Path::new("/proj"), ""), PathBuf::from("/proj"));
    }
}
