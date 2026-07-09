//! The seeded-defect canary corpus and the judge-the-judges runner (spec 13, unit 5).
//!
//! Every other read-model in this crate measures the loop's PRECISION - did the review
//! it ran uphold, did a unit converge, did a gate flap. The canary is the loop's only
//! RECALL measurement: it feeds the review panel a versioned corpus of micro-units, some
//! KNOWN-GOOD and some carrying a CATALOGED planted defect (drawn from the adversary's
//! hunt list), and scores whether the panel actually CATCHES the defects it should. It is
//! the ground truth that judges the judges.
//!
//! `rigger canary` runs the real review panel (the same lens / adversary / adjudicator
//! tiers, over the same [`AgentDriver`] port the live loop drives) against each corpus
//! item and records, per item:
//!   - **which tier caught the defect** - did tier-1 (the lenses) or tier-2 (the
//!     adversary) raise a finding about the planted defect's file;
//!   - **whether the adjudicator's verdict was correct** - reject a planted defect,
//!     approve a known-good unit;
//!   - **verdict stability under finding-order shuffling** - the same findings presented
//!     to the adjudicator in a different order must not flip the verdict (a position-bias
//!     probe).
//!
//! The scored outcomes land in the [`STREAM`] canary namespace as fold-neutral
//! [`ledger::TYPE_UNIT_STATUS`](crate::ledger::TYPE_UNIT_STATUS) events (the
//! [`STATUS_CANARY`] token, no new event type - spec 13's global constraint), so
//! [`rigger stats --canary`](crate::metrics::project_canary) reports catch rate by tier
//! without the run's own metrics fold ever seeing them (they ride a DISTINCT stream from
//! [`conductor::STREAM`](crate::conductor::STREAM)).
//!
//! Clean architecture: the runner is a use case over the [`AgentDriver`] and
//! [`EventStore`] ports and REUSES the live review authorities - [`review_protocol`]
//! (finding attribution), [`verdict_approves`] (the fail-closed gate), and
//! [`build_system_prompt`] (the discipline) - rather than a second parallel review path
//! that could drift from the one it is meant to measure.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::conductor::{
    build_system_prompt, review_protocol, verdict_approves, AgentDriver, Error, SpawnOpts,
};
use crate::config::{split_frontmatter, AgentDef, Config, ReviewPanel};
use crate::contextgraph::TYPE_REVIEW_FINDING;
use crate::eventstore::{Event, EventStore, ExpectedRevision};
use crate::ledger::TYPE_UNIT_STATUS;
use crate::spawn::{lens_role, ROLE_ADJUDICATOR, ROLE_ADVERSARY};

/// The event stream the canary run's scored outcomes land on - the canary NAMESPACE. It
/// is DISTINCT from [`conductor::STREAM`](crate::conductor::STREAM) (the run stream the
/// operator metrics fold reads), so a canary run never perturbs a project's first-pass
/// yield / review counts and `rigger stats --canary` reads only these scored outcomes.
pub const STREAM: &str = "canary";

/// The `UnitStatus` status token a per-item canary outcome rides (spec 13 forbids new
/// event types). A canary outcome is a `UnitStatus` on the canary stream, so it never
/// folds into run state - `ledger::Status::parse` returns `None` for it and the
/// run-metrics projector ignores it - exactly like the review-tier / speculation markers.
pub const STATUS_CANARY: &str = "canary";

/// The status token that OPENS one canary run (batch) on the stream. `rigger canary`
/// appends it before the batch's per-item outcomes, so `stats --canary` can scope its
/// report to the LATEST canary run (the events from the last marker onward) rather than
/// aggregating every historical run - mirroring how [`runscope::current_run`] scopes the
/// run stream by its opening `RunStarted`.
pub const STATUS_CANARY_RUN: &str = "canary-run";

/// The tier-1 label (the expert lenses, collectively) catch rate is reported for.
pub const TIER_LENS: &str = "lens";
/// The tier-2 label (the adversary) catch rate is reported for.
pub const TIER_ADVERSARY: &str = "adversary";

/// The metadata key tagging a canary outcome (and its batch marker) with the canary run
/// it belongs to.
pub const META_CANARY_BATCH: &str = "canary_batch";

/// A single canary corpus item: a micro-unit under review with its GROUND TRUTH. Parsed
/// from a `canaries/<id>.md` file - YAML frontmatter (these fields) plus a markdown body
/// (the code/diff the panel reviews), mirroring the agent-definition file shape.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct CanaryItem {
    /// The item's stable id (also the canary outcome's `UnitStatus` id).
    pub id: String,
    /// The cataloged defect class this item plants (e.g. `off-by-one`, `resource-leak`),
    /// or `none`/empty for a known-good control. Reported as the corpus's catalog.
    #[serde(default)]
    pub defect_class: String,
    /// Whether this item carries a planted defect the panel SHOULD catch. `false` is a
    /// known-good control: the panel should approve it and catch nothing.
    #[serde(default)]
    pub planted: bool,
    /// The file the planted defect lives in (and the code under review is presented as).
    /// A tier CAUGHT the defect when it raised a finding whose `about` names this file.
    #[serde(default)]
    pub anchor: String,
    /// The verdict the adjudicator SHOULD render: `reject` for a planted defect,
    /// `approve` for a known-good control. Case-insensitive; anything but `reject` is
    /// read as an expected approve.
    #[serde(default)]
    pub expected_verdict: String,
    /// The tier the corpus author expects to catch this defect (`lens` or `adversary`),
    /// or empty for a known-good control. Informational - recorded for the audit trail;
    /// the scored catch is the tier that ACTUALLY raised a finding about the anchor.
    #[serde(default)]
    pub expected_tier: String,
    /// The code/diff under review (the markdown body). Presented to every tier.
    #[serde(skip)]
    pub review: String,
}

impl CanaryItem {
    /// Whether the adjudicator SHOULD reject this item (a planted defect); the complement
    /// is an expected approve. Case-insensitive on `expected_verdict`.
    pub fn expect_reject(&self) -> bool {
        self.expected_verdict.eq_ignore_ascii_case("reject")
    }
}

/// Load the canary corpus from `dir`: every `*.md` file is parsed as YAML frontmatter
/// ([`CanaryItem`]) plus a markdown body (the code under review), returned sorted by id
/// so a canary run is deterministic. A file that is not valid frontmatter, or whose
/// `id`/`expected_verdict` is missing/invalid, fails the load loudly (a corrupt corpus
/// must not silently score as an empty or half corpus).
pub fn load_corpus(dir: &Path) -> Result<Vec<CanaryItem>, Error> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| Error(format!("canary: read corpus dir {}: {e}", dir.display())))?;
    let mut items = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| Error(format!("canary: read corpus entry: {e}")))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let bytes = std::fs::read_to_string(&path)
            .map_err(|e| Error(format!("canary: read {}: {e}", path.display())))?;
        items.push(
            parse_item(&bytes)
                .map_err(|e| Error(format!("canary: parse {}: {}", path.display(), e.0)))?,
        );
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

/// Parse one canary item from its markdown-with-YAML-frontmatter bytes: the frontmatter
/// is the [`CanaryItem`] metadata and the body is the code under review. Reuses the ONE
/// frontmatter parser [`split_frontmatter`] the agent/workflow loaders use.
fn parse_item(s: &str) -> Result<CanaryItem, Error> {
    let (front, body) = split_frontmatter(s).map_err(|e| Error(format!("frontmatter: {}", e.0)))?;
    let mut item: CanaryItem =
        serde_yaml::from_str(front).map_err(|e| Error(format!("frontmatter: {e}")))?;
    if item.id.trim().is_empty() {
        return Err(Error("canary item is missing an id".into()));
    }
    let v = item.expected_verdict.to_ascii_lowercase();
    if v != "approve" && v != "reject" {
        return Err(Error(format!(
            "canary item {:?}: expected_verdict must be \"approve\" or \"reject\", got {:?}",
            item.id, item.expected_verdict
        )));
    }
    item.review = body.trim().to_string();
    Ok(item)
}

/// The distinct planted defect classes a corpus catalogs (spec 13 unit 5 requires at
/// least three). A known-good control (`planted:false`) contributes none.
pub fn cataloged_classes(corpus: &[CanaryItem]) -> BTreeSet<String> {
    corpus
        .iter()
        .filter(|c| c.planted && !c.defect_class.trim().is_empty())
        .map(|c| c.defect_class.clone())
        .collect()
}

/// The scored outcome of running the review panel against one canary item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanaryOutcome {
    pub id: String,
    pub defect_class: String,
    pub planted: bool,
    /// Whether the adjudicator SHOULD have rejected (a planted defect).
    pub expected_reject: bool,
    /// The tier the corpus expected to catch it (informational).
    pub expected_tier: String,
    /// The tiers that ACTUALLY raised a finding about the anchor (subset of
    /// {[`TIER_LENS`], [`TIER_ADVERSARY`]}), sorted for determinism.
    pub caught_by: Vec<String>,
    /// Whether the adjudicator approved the item.
    pub verdict_approved: bool,
    /// Whether the adjudicator's verdict matched the expectation.
    pub verdict_correct: bool,
    /// Whether the adjudicator's verdict was STABLE when the findings were re-presented
    /// in a shuffled order (the position-bias probe). Trivially `true` when there are
    /// fewer than two findings to reorder.
    pub stable: bool,
}

impl CanaryOutcome {
    /// Serialize this outcome to its canary-stream event: a fold-neutral `UnitStatus`
    /// carrying the score in its data, tagged with the batch id in metadata so the fold
    /// can scope to one run.
    fn to_event(&self, batch: &str) -> Event {
        let data = json!({
            "id": self.id,
            "status": STATUS_CANARY,
            "defect_class": self.defect_class,
            "planted": self.planted,
            "expected_reject": self.expected_reject,
            "expected_tier": self.expected_tier,
            "caught_by": self.caught_by,
            "verdict_approved": self.verdict_approved,
            "verdict_correct": self.verdict_correct,
            "stable": self.stable,
        });
        Event::new(
            TYPE_UNIT_STATUS,
            serde_json::to_vec(&data).unwrap_or_default(),
        )
        .with_meta(META_CANARY_BATCH, batch)
    }

    /// Decode a canary outcome from a canary-stream event, or `None` if it is not a
    /// [`STATUS_CANARY`] `UnitStatus` (a batch marker, or a malformed event). This is the
    /// ONE wire-schema authority the metrics fold reads through, so the producer and the
    /// fold can never disagree on the shape.
    pub fn from_event(e: &Event) -> Option<CanaryOutcome> {
        if e.type_ != TYPE_UNIT_STATUS {
            return None;
        }
        let v: Value = serde_json::from_slice(&e.data).ok()?;
        if v.get("status").and_then(Value::as_str) != Some(STATUS_CANARY) {
            return None;
        }
        Some(CanaryOutcome {
            id: str_field(&v, "id"),
            defect_class: str_field(&v, "defect_class"),
            planted: bool_field(&v, "planted"),
            expected_reject: bool_field(&v, "expected_reject"),
            expected_tier: str_field(&v, "expected_tier"),
            caught_by: v
                .get("caught_by")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            verdict_approved: bool_field(&v, "verdict_approved"),
            verdict_correct: bool_field(&v, "verdict_correct"),
            stable: bool_field(&v, "stable"),
        })
    }
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn bool_field(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// The full result of one `rigger canary` invocation: the batch id it recorded under and
/// the per-item outcomes (for the CLI's summary print; the durable record is the events).
pub struct CanaryReport {
    pub batch: String,
    pub outcomes: Vec<CanaryOutcome>,
}

/// Run the review panel against every corpus item and RECORD the scored outcomes to the
/// canary [`STREAM`] (spec 13, unit 5). Each item is judged by the SAME three tiers the
/// live loop runs, over the injected [`AgentDriver`]; the scores ride fold-neutral
/// `UnitStatus` events under one batch marker.
///
/// The panel MUST name an adjudicator - the canary measures the gating verdict, so a
/// panel with none has nothing to judge and is a configuration error (mirroring
/// `validate_depth`'s mandate that every review tier name an adjudicator).
pub fn run_canary(
    store: &dyn EventStore,
    driver: &dyn AgentDriver,
    cfg: &Config,
    panel: &ReviewPanel,
    corpus: &[CanaryItem],
) -> Result<CanaryReport, Error> {
    if panel.adjudicator.is_empty() {
        return Err(Error(
            "canary: the review panel names no adjudicator - the canary measures the \
             gating verdict, so an adjudicator is required"
                .into(),
        ));
    }
    let batch = uuid::Uuid::new_v4().to_string();
    // Open the batch with its marker so the fold can scope to this run.
    append(
        store,
        Event::new(
            TYPE_UNIT_STATUS,
            serde_json::to_vec(&json!({"id": batch, "status": STATUS_CANARY_RUN}))
                .unwrap_or_default(),
        )
        .with_meta(META_CANARY_BATCH, &batch),
    )?;
    let mut outcomes = Vec::new();
    for item in corpus {
        let outcome = score_item(driver, cfg, panel, item)?;
        append(store, outcome.to_event(&batch))?;
        outcomes.push(outcome);
    }
    Ok(CanaryReport { batch, outcomes })
}

fn append(store: &dyn EventStore, event: Event) -> Result<(), Error> {
    store.append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&event))?;
    Ok(())
}

/// A finding a reviewer raised during a canary spawn, reduced to what scoring needs: the
/// files it is `about` (to decide catch) and its one-line `summary` (to present to the
/// adjudicator in the position-bias probe).
#[derive(Clone, Debug)]
struct Finding {
    about: Vec<String>,
    summary: String,
}

impl Finding {
    /// Whether this finding names `item`'s planted-defect file - the catch signal.
    fn catches(&self, item: &CanaryItem) -> bool {
        !item.anchor.is_empty() && self.about.iter().any(|f| f == &item.anchor)
    }
}

/// Score one canary item: run the lenses (tier 1) and the adversary (tier 2) collecting
/// their findings, then the adjudicator (tier 3) twice - once with the findings in
/// natural order and once reversed - to judge the verdict AND probe it for position bias.
fn score_item(
    driver: &dyn AgentDriver,
    cfg: &Config,
    panel: &ReviewPanel,
    item: &CanaryItem,
) -> Result<CanaryOutcome, Error> {
    let mut caught = BTreeSet::new();
    let mut findings: Vec<Finding> = Vec::new();

    // TIER 1: the expert lenses, collectively one tier. Any lens raising a finding about
    // the anchor catches the defect for the lens tier.
    for lens in &panel.lenses {
        let raised = run_review_tier(driver, cfg, item, lens, &lens_role(lens))?;
        if raised.iter().any(|f| f.catches(item)) {
            caught.insert(TIER_LENS.to_string());
        }
        findings.extend(raised);
    }

    // TIER 2: the adversary, holding the lenses to a higher bar.
    if !panel.adversary.is_empty() {
        let raised = run_review_tier(driver, cfg, item, &panel.adversary, ROLE_ADVERSARY)?;
        if raised.iter().any(|f| f.catches(item)) {
            caught.insert(TIER_ADVERSARY.to_string());
        }
        findings.extend(raised);
    }

    // TIER 3: the adjudicator renders the gating verdict, judged through the SAME
    // fail-closed authority the live loop uses. Position-bias probe: re-present the same
    // findings reversed - a verdict that flips on order alone is unstable.
    let ordered: Vec<&Finding> = findings.iter().collect();
    let approved = adjudicate(driver, cfg, panel, item, &ordered, "a")?;
    let stable = if ordered.len() < 2 {
        // Nothing to reorder - position bias is not probeable, so it is trivially stable
        // (and re-running would only re-present the identical single/zero-finding prompt).
        true
    } else {
        let mut reversed: Vec<&Finding> = ordered.clone();
        reversed.reverse();
        let approved_reversed = adjudicate(driver, cfg, panel, item, &reversed, "b")?;
        approved == approved_reversed
    };

    let expected_reject = item.expect_reject();
    // Correct iff the adjudicator's approve/reject matches the expectation: a planted
    // defect (expected_reject) must NOT be approved, a known-good control must be.
    let verdict_correct = approved != expected_reject;
    Ok(CanaryOutcome {
        id: item.id.clone(),
        defect_class: item.defect_class.clone(),
        planted: item.planted,
        expected_reject,
        expected_tier: item.expected_tier.clone(),
        caught_by: caught.into_iter().collect(),
        verdict_approved: approved,
        verdict_correct,
        stable,
    })
}

/// Run one review TIER (a lens or the adversary) against a canary item and collect the
/// findings it emits. The reviewer receives the item's code under review plus the SAME
/// [`review_protocol`] the live loop appends, so it attributes each finding by its role
/// token; the emit callback captures every `ReviewFinding` in process (the cli driver
/// bridges a subprocess reviewer's stdout findings through the same callback).
fn run_review_tier(
    driver: &dyn AgentDriver,
    cfg: &Config,
    item: &CanaryItem,
    agent_id: &str,
    role: &str,
) -> Result<Vec<Finding>, Error> {
    let agent = agent_of(cfg, agent_id, role)?;
    let prompt = format!("{}\n\n{}", review_header(item), review_protocol(role));
    let opts = canary_opts(item, role, agent);
    let (_output, findings) = spawn_collecting(driver, agent, &prompt, &opts)?;
    Ok(findings)
}

/// Run the adjudicator against a canary item with the collected findings presented in the
/// given order, and return whether it APPROVED (via the fail-closed [`verdict_approves`]).
/// `ordinal` distinguishes the natural-order and reversed-order probe spawns by id.
fn adjudicate(
    driver: &dyn AgentDriver,
    cfg: &Config,
    panel: &ReviewPanel,
    item: &CanaryItem,
    findings: &[&Finding],
    ordinal: &str,
) -> Result<bool, Error> {
    let agent = agent_of(cfg, &panel.adjudicator, ROLE_ADJUDICATOR)?;
    let prompt = adjudicator_prompt(item, findings);
    let mut opts = canary_opts(item, ROLE_ADJUDICATOR, agent);
    // Distinguish the two probe spawns (natural vs reversed order) by id.
    opts.id = format!("{}:{ordinal}", opts.id);
    let (output, _findings) = spawn_collecting(driver, agent, &prompt, &opts)?;
    Ok(verdict_approves(&output))
}

/// Look up a canary reviewer's agent definition, erroring clearly when the panel names an
/// agent the config does not define.
fn agent_of<'a>(cfg: &'a Config, agent_id: &str, role: &str) -> Result<&'a AgentDef, Error> {
    cfg.agents.get(agent_id).ok_or_else(|| {
        Error(format!(
            "canary: panel {role} references unknown agent {agent_id:?}"
        ))
    })
}

/// The spawn options for a canary reviewer: a deterministic per-item, per-role id and the
/// discipline-composed system prompt. A canary reviewer owns no worktree - it reviews the
/// corpus snippet in the prompt - so it runs with no isolation and an empty blast radius.
fn canary_opts(item: &CanaryItem, role: &str, agent: &AgentDef) -> SpawnOpts {
    SpawnOpts {
        id: format!("canary:{}:{role}", item.id),
        unit: format!("canary:{}", item.id),
        stage: "canary".to_string(),
        attempt: 0,
        system_prompt: build_system_prompt(&agent.prompt),
        dir: String::new(),
        isolation: false,
        parallel: false,
        blast_radius: Vec::new(),
        run_id: String::new(),
    }
}

/// Spawn a reviewer and collect every `ReviewFinding` it emits (the lens/adversary work
/// channel), returning its stdout and the collected findings. Reuses the emit-callback
/// seam the live review path uses, so this works on every driver: a fake test driver
/// calls `emit` directly, the cli driver bridges a subprocess's stdout findings, and the
/// workflow driver emits them live.
fn spawn_collecting(
    driver: &dyn AgentDriver,
    agent: &AgentDef,
    prompt: &str,
    opts: &SpawnOpts,
) -> Result<(String, Vec<Finding>), Error> {
    let findings = RefCell::new(Vec::new());
    let emit = |t: &str, v: Value| -> Result<(), Error> {
        if t == TYPE_REVIEW_FINDING {
            let about = v
                .get("about")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let summary = v
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            findings.borrow_mut().push(Finding { about, summary });
        }
        Ok(())
    };
    let result = driver.spawn(agent, prompt, opts, &emit)?;
    Ok((result.output, findings.into_inner()))
}

/// The header presented to every tier: names the file under review (so a reviewer that
/// finds a defect attributes its finding to that file) and frames the snippet.
fn review_header(item: &CanaryItem) -> String {
    let file = if item.anchor.is_empty() {
        "the code below"
    } else {
        &item.anchor
    };
    format!(
        "You are reviewing a micro-unit. The code under review is the file `{file}`. \
         Review it for defects and raise a finding about `{file}` for each one you find.\n\n\
         ----- BEGIN {file} -----\n{}\n----- END {file} -----",
        item.review
    )
}

/// The adjudicator's prompt: the code under review plus the tiers' findings presented in
/// the given ORDER (the position-bias probe reorders this list), and the fail-closed
/// verdict instruction [`verdict_approves`] reads.
fn adjudicator_prompt(item: &CanaryItem, findings: &[&Finding]) -> String {
    let mut block = String::new();
    if findings.is_empty() {
        block.push_str("(no findings were raised)\n");
    } else {
        for (i, f) in findings.iter().enumerate() {
            let about = f.about.join(", ");
            block.push_str(&format!("{}. [{about}] {}\n", i + 1, f.summary));
        }
    }
    format!(
        "{}\n\nThe review tiers raised these findings, in order:\n{block}\n\
         Render your gating verdict as a single JSON line: {{\"verdict\":\"approve\"}} to \
         integrate, or {{\"verdict\":\"reject\"}} to send it back.",
        review_header(item)
    )
}

/// Scope `events` (a canary-stream read) to the LATEST canary run: the slice from the
/// last [`STATUS_CANARY_RUN`] batch marker onward, or the whole slice when none is present
/// (a legacy or marker-less store). Mirrors [`runscope::current_run`] for the run stream.
pub fn latest_run(events: &[Event]) -> &[Event] {
    match events.iter().rposition(is_batch_marker) {
        Some(i) => &events[i..],
        None => events,
    }
}

fn is_batch_marker(e: &Event) -> bool {
    if e.type_ != TYPE_UNIT_STATUS {
        return false;
    }
    serde_json::from_slice::<Value>(&e.data)
        .ok()
        .and_then(|v| {
            v.get("status")
                .and_then(Value::as_str)
                .map(|s| s == STATUS_CANARY_RUN)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor::AgentResult;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::Direction;

    fn agent(id: &str) -> AgentDef {
        AgentDef {
            id: id.to_string(),
            ..Default::default()
        }
    }

    fn panel() -> ReviewPanel {
        ReviewPanel {
            lenses: vec!["sdet".into()],
            adversary: "adv".into(),
            adjudicator: "adj".into(),
            tiers: None,
        }
    }

    fn cfg() -> Config {
        let mut c = Config::default();
        for id in ["sdet", "adv", "adj"] {
            c.agents.insert(id.to_string(), agent(id));
        }
        c
    }

    /// A scripted driver modelling a review panel. A lens/adversary raises a CRITICAL
    /// finding ABOUT the anchor only when its own tier is the `catching_tier` AND the item
    /// (identified by its anchor) is one of `planted_anchors`; otherwise it raises a benign
    /// off-anchor nit. The adjudicator rejects when it sees a critical finding - either
    /// anywhere (`order_sensitive:false`, stable) or only in the FIRST position
    /// (`order_sensitive:true`, position-biased).
    struct Scripted {
        catching_tier: &'static str, // TIER_LENS | TIER_ADVERSARY | "" (nobody catches)
        planted_anchors: Vec<String>,
        adjudicator_order_sensitive: bool,
    }

    impl AgentDriver for Scripted {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id == "adj" {
                let reject = if self.adjudicator_order_sensitive {
                    first_finding_is_critical(prompt)
                } else {
                    any_finding_is_critical(prompt)
                };
                let verdict = if reject { "reject" } else { "approve" };
                return Ok(AgentResult {
                    output: format!("{{\"verdict\":\"{verdict}\"}}"),
                    resolved_model: String::new(),
                });
            }
            // The anchor the review header names, between the first pair of backticks.
            let anchor = prompt
                .split_once('`')
                .and_then(|(_, rest)| rest.split_once('`'))
                .map(|(a, _)| a.to_string())
                .unwrap_or_default();
            let my_tier = if a.id == "adv" {
                TIER_ADVERSARY
            } else {
                TIER_LENS
            };
            let catches = my_tier == self.catching_tier && self.planted_anchors.contains(&anchor);
            let finding = if catches {
                json!({"id": format!("f-{}", opts.id), "by": "x", "summary": "CRIT defect here", "about": [anchor]})
            } else {
                json!({"id": format!("f-{}", opts.id), "by": "x", "summary": "minor style nit", "about": ["other.rs"]})
            };
            emit(TYPE_REVIEW_FINDING, finding)?;
            Ok(AgentResult {
                output: "reviewed".into(),
                resolved_model: String::new(),
            })
        }
    }

    fn first_finding_is_critical(prompt: &str) -> bool {
        prompt
            .lines()
            .find(|l| l.trim_start().starts_with("1. ["))
            .map(|l| l.contains("CRIT"))
            .unwrap_or(false)
    }

    fn any_finding_is_critical(prompt: &str) -> bool {
        prompt.contains("CRIT defect here")
    }

    fn item(id: &str, class: &str, planted: bool, verdict: &str, tier: &str) -> CanaryItem {
        CanaryItem {
            id: id.into(),
            defect_class: class.into(),
            planted,
            anchor: format!("{id}.rs"),
            expected_verdict: verdict.into(),
            expected_tier: tier.into(),
            review: format!("fn {id}() {{}}"),
        }
    }

    #[test]
    fn parse_item_reads_frontmatter_and_body_and_rejects_a_bad_verdict() {
        let good = "---\nid: off-by-one\ndefect_class: off-by-one\nplanted: true\nanchor: src/sum.rs\nexpected_verdict: reject\nexpected_tier: adversary\n---\nfn sum() { for i in 0..=n {} }\n";
        let it = parse_item(good).unwrap();
        assert_eq!(it.id, "off-by-one");
        assert_eq!(it.defect_class, "off-by-one");
        assert!(it.planted);
        assert_eq!(it.anchor, "src/sum.rs");
        assert!(it.expect_reject());
        assert_eq!(it.review, "fn sum() { for i in 0..=n {} }");

        // A missing/invalid expected_verdict fails the load loudly.
        let bad = "---\nid: x\nexpected_verdict: maybe\n---\nbody\n";
        assert!(parse_item(bad).is_err());
        // A missing id fails too.
        let noid = "---\nexpected_verdict: approve\n---\nbody\n";
        assert!(parse_item(noid).is_err());
    }

    #[test]
    fn a_planted_defect_caught_by_the_adversary_scores_a_correct_reject() {
        // The lens misses; the adversary catches. A planted defect, so the correct verdict
        // is reject. The adjudicator (order-insensitive) rejects on the critical finding.
        let driver = Scripted {
            catching_tier: TIER_ADVERSARY,
            planted_anchors: vec!["leak.rs".into()],
            adjudicator_order_sensitive: false,
        };
        let it = item("leak", "resource-leak", true, "reject", "adversary");
        let outcome = score_item(&driver, &cfg(), &panel(), &it).unwrap();
        assert_eq!(outcome.caught_by, vec![TIER_ADVERSARY.to_string()]);
        assert!(
            !outcome.verdict_approved,
            "a planted defect must be rejected"
        );
        assert!(
            outcome.verdict_correct,
            "reject of a planted defect is correct"
        );
        assert!(outcome.stable, "an order-insensitive adjudicator is stable");
    }

    #[test]
    fn a_known_good_unit_scores_a_correct_approve_and_no_catch() {
        // The item is not in planted_anchors, so no tier raises a critical finding; the
        // adjudicator sees only benign nits and approves.
        let driver = Scripted {
            catching_tier: TIER_ADVERSARY,
            planted_anchors: vec![], // clean.rs is NOT planted
            adjudicator_order_sensitive: false,
        };
        let it = item("clean", "none", false, "approve", "");
        let outcome = score_item(&driver, &cfg(), &panel(), &it).unwrap();
        assert!(
            outcome.caught_by.is_empty(),
            "a known-good unit catches nothing"
        );
        assert!(outcome.verdict_approved);
        assert!(
            outcome.verdict_correct,
            "approve of a known-good unit is correct"
        );
    }

    #[test]
    fn an_order_sensitive_adjudicator_is_scored_unstable() {
        // The adversary raises the CRITICAL finding (about the anchor); the lens raises a
        // benign nit. Presented natural (benign first) the order-sensitive adjudicator
        // sees no critical FIRST and approves; reversed (critical first) it rejects - the
        // position-bias probe catches the flip.
        let driver = Scripted {
            catching_tier: TIER_ADVERSARY,
            planted_anchors: vec!["offbyone.rs".into()],
            adjudicator_order_sensitive: true,
        };
        let it = item("offbyone", "off-by-one", true, "reject", "adversary");
        let outcome = score_item(&driver, &cfg(), &panel(), &it).unwrap();
        assert!(
            !outcome.stable,
            "a verdict that flips on finding order must be scored unstable"
        );
    }

    #[test]
    fn run_canary_records_a_batch_and_one_outcome_per_item_in_the_canary_stream() {
        let store = Store::open(":memory:").unwrap();
        let driver = Scripted {
            catching_tier: TIER_ADVERSARY,
            planted_anchors: vec!["leak.rs".into()],
            adjudicator_order_sensitive: false,
        };
        let corpus = vec![
            item("leak", "resource-leak", true, "reject", "adversary"),
            item("clean", "none", false, "approve", ""),
        ];
        let report = run_canary(&store, &driver, &cfg(), &panel(), &corpus).unwrap();
        assert_eq!(report.outcomes.len(), 2);

        // The canary stream carries the batch marker + one outcome per item.
        let canary = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(canary.len(), 3, "one batch marker + two outcomes");
        assert!(is_batch_marker(&canary[0]));
        let outcomes: Vec<CanaryOutcome> = canary
            .iter()
            .filter_map(CanaryOutcome::from_event)
            .collect();
        assert_eq!(outcomes.len(), 2);

        // Nothing lands on the run stream - a canary run is fully isolated.
        let run = store
            .read_stream(crate::conductor::STREAM, 0, Direction::Forward)
            .unwrap();
        assert!(run.is_empty(), "a canary run never writes the run stream");

        // The recorded outcomes round-trip through the wire schema.
        let leak = outcomes.iter().find(|o| o.id == "leak").unwrap();
        assert!(leak.planted);
        assert_eq!(leak.caught_by, vec![TIER_ADVERSARY.to_string()]);
        assert!(leak.verdict_correct);
        let clean = outcomes.iter().find(|o| o.id == "clean").unwrap();
        assert!(clean.verdict_approved && clean.verdict_correct);
    }

    #[test]
    fn run_canary_requires_an_adjudicator() {
        let store = Store::open(":memory:").unwrap();
        let driver = Scripted {
            catching_tier: "",
            planted_anchors: vec![],
            adjudicator_order_sensitive: false,
        };
        let mut p = panel();
        p.adjudicator = String::new();
        let corpus = vec![item("x", "off-by-one", true, "reject", "lens")];
        assert!(run_canary(&store, &driver, &cfg(), &p, &corpus).is_err());
    }

    #[test]
    fn cataloged_classes_counts_only_planted_distinct_classes() {
        let corpus = vec![
            item("a", "off-by-one", true, "reject", "lens"),
            item("b", "resource-leak", true, "reject", "adversary"),
            item("c", "off-by-one", true, "reject", "lens"), // dup class
            item("d", "none", false, "approve", ""),         // known-good, no class
        ];
        let classes = cataloged_classes(&corpus);
        assert_eq!(classes.len(), 2);
        assert!(classes.contains("off-by-one"));
        assert!(classes.contains("resource-leak"));
    }

    #[test]
    fn the_shipped_corpus_loads_and_catalogs_at_least_three_defect_classes() {
        // The Done-when bar (spec 13, unit 5): the versioned corpus under `canaries/`
        // catalogs at least three planted defect classes, and every item is well-formed
        // (loads through the strict loader, names an anchor, and carries code to review).
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("canaries");
        let corpus = load_corpus(&dir).expect("the shipped canary corpus must load");
        assert!(!corpus.is_empty(), "the shipped corpus must have items");
        let classes = cataloged_classes(&corpus);
        assert!(
            classes.len() >= 3,
            "the shipped corpus must catalog at least three planted defect classes; got {classes:?}"
        );
        // It must also carry at least one known-good control (the false-positive anchor).
        assert!(
            corpus.iter().any(|c| !c.planted),
            "the corpus must include a known-good control"
        );
        for c in &corpus {
            assert!(
                !c.anchor.trim().is_empty(),
                "canary {:?} has no anchor",
                c.id
            );
            assert!(
                !c.review.trim().is_empty(),
                "canary {:?} has no code under review",
                c.id
            );
            // A planted item expects a reject and names a defect class; a control expects
            // an approve. The loader already enforced a valid verdict token.
            if c.planted {
                assert!(
                    c.expect_reject(),
                    "planted canary {:?} must expect reject",
                    c.id
                );
                assert!(
                    !c.defect_class.trim().is_empty() && c.defect_class != "none",
                    "planted canary {:?} must name a defect class",
                    c.id
                );
            } else {
                assert!(
                    !c.expect_reject(),
                    "known-good canary {:?} must expect approve",
                    c.id
                );
            }
        }
    }

    #[test]
    fn latest_run_scopes_to_the_last_batch_marker() {
        let marker = || {
            Event::new(
                TYPE_UNIT_STATUS,
                serde_json::to_vec(&json!({"id":"b","status":STATUS_CANARY_RUN})).unwrap(),
            )
        };
        let outcome = |id: &str| {
            CanaryOutcome {
                id: id.into(),
                defect_class: "off-by-one".into(),
                planted: true,
                expected_reject: true,
                expected_tier: "lens".into(),
                caught_by: vec![TIER_LENS.into()],
                verdict_approved: false,
                verdict_correct: true,
                stable: true,
            }
            .to_event("b")
        };
        let events = vec![marker(), outcome("old"), marker(), outcome("new")];
        let scoped = latest_run(&events);
        let ids: Vec<String> = scoped
            .iter()
            .filter_map(CanaryOutcome::from_event)
            .map(|o| o.id)
            .collect();
        assert_eq!(
            ids,
            vec!["new".to_string()],
            "only the latest run is scoped"
        );
    }
}
