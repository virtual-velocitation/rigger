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
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

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
    /// The count of findings each tier RAISED for this item - regardless of whether any
    /// of them caught the planted defect - keyed by tier label ([`TIER_LENS`],
    /// [`TIER_ADVERSARY`]). This is the over-flagging / findings-volume measure: a tier
    /// that raises many findings but catches nothing is scored honestly on both axes. A
    /// tier that ran (the adversary is optional per panel) but raised nothing records `0`,
    /// never an absent key - the same "seed every known tier" discipline `tier_catch`
    /// follows, so a silent key omission never reads as "not measured".
    pub findings_raised: BTreeMap<String, u64>,
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
            "findings_raised": self.findings_raised,
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
            findings_raised: v
                .get("findings_raised")
                .and_then(Value::as_object)
                .map(|obj| {
                    obj.iter()
                        .map(|(k, val)| (k.clone(), val.as_u64().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default(),
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
///
/// `jobs` is the TOTAL concurrent-spawn budget (spec 61, ITEM SHARDING AND THE JOBS CAP):
/// [`spawn_budget`] splits it between this function's own OUTER per-item sharding and the
/// LENS FAN-OUT criterion's already-built INNER `score_item` fan-out, so their PRODUCT -
/// the worst-case number of review-panel spawns in flight at once - never exceeds it. A
/// caller that does not want to choose passes [`default_jobs`].
///
/// `on_item` is called once per corpus item, the INSTANT that item's own score completes -
/// possibly from a worker thread other than the caller's, and possibly while a sibling
/// item's score is still in flight (spec 61, PROGRESS). This is a genuine per-item
/// STREAMING hook, not a batched one: it fires from inside the `map_ordered` closure below,
/// before `map_ordered` has joined every worker, so a caller observing it sees each item as
/// it actually finishes rather than every item's outcome arriving in one simultaneous burst
/// once the whole corpus is done. `rigger canary` (`cmd_canary`) wires this to a stdout
/// print; a caller with nothing to report passes a no-op.
pub fn run_canary(
    store: &dyn EventStore,
    driver: &dyn AgentDriver,
    cfg: &Config,
    panel: &ReviewPanel,
    corpus: &[CanaryItem],
    jobs: usize,
    on_item: &(dyn Fn(&CanaryOutcome, Duration) + Sync),
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
    // Independent corpus items shard across item_workers; each item's tier-1 lens fan-out
    // (score_item's own, already-built concurrency) gets lens_workers - a CONSUMER
    // relationship: this call site decides the width, score_item only accepts whatever it
    // is given. map_ordered is index-preserving (chunk-order concatenation), so `scored`
    // reads in the SAME order as `corpus` regardless of which item's spawns happened to
    // finish first - the aggregated report and the events appended below are therefore
    // identical to a fully serial run's, satisfying the determinism constraint (pinned by
    // `run_canary_scores_identically_regardless_of_the_jobs_width`). Every item is scored
    // (map_ordered's documented run-to-completion contract, the same choice the LENS
    // FAN-OUT criterion already made at its own seam); the FIRST error among them, if any,
    // is surfaced by the `?` below only after every item has been scored, never mid-flight.
    let (item_workers, lens_workers) = spawn_budget(jobs, corpus.len());
    let (scored, _engaged) = crate::parallel::map_ordered(corpus, item_workers, |item| {
        let start = Instant::now();
        let outcome = score_item(driver, cfg, panel, item, lens_workers);
        // Fire the progress hook the INSTANT this item's own score completes, on
        // whichever worker thread just finished it - NOT after map_ordered has joined
        // every item_workers thread. The aggregation loop below only ever sees the
        // full, ordered Vec once every worker has returned, so hooking there would
        // bunch every item's line into one simultaneous end-of-run burst instead of
        // streaming them as they genuinely finish (the defect PROGRESS exists to fix,
        // spec 61 Goal #3). A scoring error is not a completed item - nothing to report.
        if let Ok(o) = &outcome {
            on_item(o, start.elapsed());
        }
        outcome
    });
    let mut outcomes = Vec::with_capacity(scored.len());
    for outcome in scored {
        let outcome = outcome?;
        append(store, outcome.to_event(&batch))?;
        outcomes.push(outcome);
    }
    Ok(CanaryReport { batch, outcomes })
}

/// The default `--jobs` total concurrent-spawn budget `rigger canary` uses when the
/// operator does not name one: the crate-wide default worker width
/// ([`crate::parallel::default_workers`], the ONE default-width authority - reused, not
/// re-decided here), floored at 2 so the default is ALWAYS greater than one (spec 61's
/// Done-when text) regardless of what a constrained host's core count reports - unlike
/// [`crate::parallel::default_workers`] itself, which may honestly report 1.
pub fn default_jobs() -> usize {
    crate::parallel::default_workers().max(2)
}

/// Split a total `jobs` spawn-concurrency budget between [`run_canary`]'s two concurrency
/// dimensions - `item_workers` (independent corpus items sharded across this function's
/// caller) and `lens_workers` (each item's tier-1 lens fan-out, the LENS FAN-OUT
/// criterion's inner `score_item` seam) - so their PRODUCT, the worst-case count of
/// review-panel spawns ever in flight at once, never exceeds `jobs`.
///
/// `item_workers` is `min(jobs, corpus_len)` (never more items sharded than exist - a
/// `--jobs` value larger than the corpus must not spawn idle item threads, the "sane for
/// the corpus size" spec text), and `lens_workers` is the REMAINING budget,
/// `jobs / item_workers` by floor division - which is why the product bound holds for
/// every input: `item_workers * (jobs / item_workers) <= jobs` always, by the definition
/// of floor division. `jobs == 0` and `corpus_len == 0` both degrade to a width of 1 (no
/// caller-visible zero-width pool - `map_ordered` itself already treats `workers <= 1` as
/// its serial oracle).
fn spawn_budget(jobs: usize, corpus_len: usize) -> (usize, usize) {
    let jobs = jobs.max(1);
    let item_workers = jobs.min(corpus_len.max(1));
    let lens_workers = (jobs / item_workers).max(1);
    (item_workers, lens_workers)
}

fn append(store: &dyn EventStore, event: Event) -> Result<(), Error> {
    store
        .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&event))?
        .one(&format!("the {} of this canary batch", event.type_))?;
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
    /// Whether this finding names `item`'s planted-defect file - the catch signal. Matching
    /// is TOLERANT of how a live reviewer spells the same file (see [`paths_match`]).
    fn catches(&self, item: &CanaryItem) -> bool {
        !item.anchor.is_empty() && self.about.iter().any(|f| paths_match(f, &item.anchor))
    }
}

/// Whether `about` (a reviewer's free-form file reference) and `anchor` (the corpus item's
/// planted-defect file) name the same file under TOLERANT spelling: exact equality, or one
/// is a path-segment-boundary SUFFIX of the other. This covers an absolute path naming a
/// repo-relative anchor's tail (`/home/dev/repo/src/sum.rs` vs `src/sum.rs`) and a shorter
/// relative/basename spelling naming the anchor's tail in reverse (`a.rs` vs
/// `corpus/a.rs`). The tolerance is BOUNDED at segment boundaries, never raw substring:
/// `extra.rs` is not a match for anchor `a.rs` even though it ends with that text, because
/// the byte immediately before the shared tail is not `/`.
///
/// `about` must be non-empty: `str::strip_suffix("")` trivially succeeds on ANY haystack,
/// so without this guard an empty `about` (a malformed live finding, or an unvalidated
/// corpus `anchor:` typo landing on a bare directory path) would invert into a spurious
/// match whenever `anchor` happens to end in `/` - a fake catch bearing zero relation to
/// what was actually found. Mirrors the same-shaped guard [`Finding::catches`] already
/// applies to `anchor` being empty.
fn paths_match(about: &str, anchor: &str) -> bool {
    !about.is_empty()
        && (about == anchor
            || about
                .strip_suffix(anchor)
                .is_some_and(|prefix| prefix.ends_with('/'))
            || anchor
                .strip_suffix(about)
                .is_some_and(|prefix| prefix.ends_with('/')))
}

/// Score one canary item: run the lenses (tier 1) and the adversary (tier 2) collecting
/// their findings, then the adjudicator (tier 3) twice - once with the findings in
/// natural order and once reversed - to judge the verdict AND probe it for position bias.
///
/// `lens_workers` is the pool width `crate::parallel::map_ordered` fans the tier-1 lenses
/// out over - the scheduling seam the LENS FAN-OUT criterion pins with a barrier-gated
/// test driver. It is index-preserving (chunk-order concatenation), so the aggregation
/// below reads in the SAME order the serial walk it replaces would have produced, and a
/// caller passing `1` gets that exact serial walk back (the seam's own oracle). The
/// ITEM SHARDING criterion is the one caller that turns this into the shared `--jobs`
/// total-concurrent-spawn budget; this function only accepts whatever width it is given.
fn score_item(
    driver: &dyn AgentDriver,
    cfg: &Config,
    panel: &ReviewPanel,
    item: &CanaryItem,
    lens_workers: usize,
) -> Result<CanaryOutcome, Error> {
    let mut caught = BTreeSet::new();
    let mut findings: Vec<Finding> = Vec::new();
    // The findings-volume measure (FINDINGS VOLUME criterion): how many findings each tier
    // RAISED this item, regardless of catch. Seeded at 0 for both known tiers up front - the
    // same "seed every known tier" discipline `project_canary`'s `tier_catch` seeding follows
    // - so a tier that ran and raised nothing records an honest 0, never an absent key.
    let mut findings_raised: BTreeMap<String, u64> = BTreeMap::new();
    findings_raised.insert(TIER_LENS.to_string(), 0);
    findings_raised.insert(TIER_ADVERSARY.to_string(), 0);

    // TIER 1: the expert lenses, collectively one tier, run CONCURRENTLY over
    // lens_workers. Any lens raising a finding about the anchor catches the defect for
    // the lens tier; aggregation reads the (order-preserved) results in the same order
    // the serial for-loop it replaces did, so the scored outcome cannot depend on which
    // lens spawn happened to finish first.
    let (lens_results, _engaged) =
        crate::parallel::map_ordered(&panel.lenses, lens_workers, |lens| {
            run_review_tier(driver, cfg, item, lens, &lens_role(lens))
        });
    for raised in lens_results {
        let raised = raised?;
        if raised.iter().any(|f| f.catches(item)) {
            caught.insert(TIER_LENS.to_string());
        }
        *findings_raised.entry(TIER_LENS.to_string()).or_insert(0) += raised.len() as u64;
        findings.extend(raised);
    }

    // TIER 2: the adversary, holding the lenses to a higher bar.
    if !panel.adversary.is_empty() {
        let raised = run_review_tier(driver, cfg, item, &panel.adversary, ROLE_ADVERSARY)?;
        if raised.iter().any(|f| f.catches(item)) {
            caught.insert(TIER_ADVERSARY.to_string());
        }
        *findings_raised
            .entry(TIER_ADVERSARY.to_string())
            .or_insert(0) += raised.len() as u64;
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
        findings_raised,
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
        // A canary is a synthetic self-test, not a unit doing spec work, so it carries no
        // live work-line (spec 19a, c4): an empty title stays byte-identical on the wire.
        title: String::new(),
        // A canary reviewer runs no build of its own (it judges a corpus snippet already
        // in the prompt, isolation: false, no worktree) - the shared build environment
        // (spec 65) has nothing to reach here, so this stays empty rather than resolving
        // config this spawn never uses.
        env: Vec::new(),
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

    fn with_anchor(anchor: &str) -> CanaryItem {
        CanaryItem {
            anchor: anchor.into(),
            ..Default::default()
        }
    }

    fn finding(about: &[&str]) -> Finding {
        Finding {
            about: about.iter().map(|s| s.to_string()).collect(),
            summary: String::new(),
        }
    }

    #[test]
    fn catches_matches_an_absolute_path_spelling_of_a_repo_relative_anchor() {
        let it = with_anchor("src/sum.rs");
        assert!(
            finding(&["/home/dev/repo/src/sum.rs"]).catches(&it),
            "an absolute path ending in the anchor at a segment boundary must catch"
        );
    }

    #[test]
    fn catches_matches_a_segment_boundary_path_suffix_in_either_direction() {
        // The finding names a shorter suffix spelling of a longer anchor.
        let it = with_anchor("corpus/a.rs");
        assert!(
            finding(&["a.rs"]).catches(&it),
            "a basename spelling that is the anchor's own segment-boundary suffix must catch"
        );

        // The anchor is itself the shorter suffix spelling of a longer `about` entry.
        let it2 = with_anchor("a.rs");
        assert!(
            finding(&["corpus/a.rs"]).catches(&it2),
            "corpus/a.rs is a.rs's segment-boundary suffix spelling and must catch"
        );
    }

    #[test]
    fn catches_rejects_a_name_that_merely_ends_with_the_anchors_text() {
        let it = with_anchor("a.rs");
        assert!(
            !finding(&["extra.rs"]).catches(&it),
            "extra.rs ends with a.rs's text but not at a path-segment boundary - must not catch"
        );
        assert!(
            !finding(&["other.rs"]).catches(&it),
            "an unrelated file must not catch"
        );
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
        let outcome = score_item(&driver, &cfg(), &panel(), &it, 1).unwrap();
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
        let outcome = score_item(&driver, &cfg(), &panel(), &it, 1).unwrap();
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
        let outcome = score_item(&driver, &cfg(), &panel(), &it, 1).unwrap();
        assert!(
            !outcome.stable,
            "a verdict that flips on finding order must be scored unstable"
        );
    }

    /// A driver that blocks every LENS spawn (any agent id that is not the adversary or
    /// the adjudicator) on a shared barrier before delegating to a real `Scripted` driver's
    /// scoring logic. If the lens tier still ran one spawn at a time, the first lens's
    /// `wait()` would never see its siblings arrive and the test would hang - reaching the
    /// assertions below at all is the proof that the lenses were in flight together,
    /// mirroring `map_ordered_engages_every_worker_deterministically`'s barrier proof.
    struct BarrierGatedLenses {
        barrier: std::sync::Barrier,
        inner: Scripted,
    }

    impl AgentDriver for BarrierGatedLenses {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id != "adv" && a.id != "adj" {
                self.barrier.wait();
            }
            self.inner.spawn(a, prompt, opts, emit)
        }
    }

    fn cfg_for(ids: &[&str]) -> Config {
        let mut c = Config::default();
        for id in ids {
            c.agents.insert((*id).to_string(), agent(id));
        }
        c
    }

    fn panel_with_lenses(lenses: &[&str]) -> ReviewPanel {
        ReviewPanel {
            lenses: lenses.iter().map(|s| (*s).to_string()).collect(),
            adversary: "adv".into(),
            adjudicator: "adj".into(),
            tiers: None,
        }
    }

    #[test]
    fn lens_tier_fans_out_concurrently_at_the_scheduling_seam() {
        // Three lenses, a barrier of three: score_item must have all three lens spawns
        // in flight simultaneously or this deadlocks. lens_workers pins the seam so the
        // proof does not depend on how many cores the test host happens to have.
        let lenses = ["lens-a", "lens-b", "lens-c"];
        let driver = BarrierGatedLenses {
            barrier: std::sync::Barrier::new(lenses.len()),
            inner: Scripted {
                catching_tier: TIER_LENS,
                planted_anchors: vec!["fanout.rs".into()],
                adjudicator_order_sensitive: false,
            },
        };
        let mut ids: Vec<&str> = lenses.to_vec();
        ids.extend(["adv", "adj"]);
        let c = cfg_for(&ids);
        let p = panel_with_lenses(&lenses);
        let it = item("fanout", "off-by-one", true, "reject", "lens");

        let outcome = score_item(&driver, &c, &p, &it, lenses.len()).unwrap();
        assert_eq!(
            outcome.caught_by,
            vec![TIER_LENS.to_string()],
            "the fanned-out lens tier still scores the catch"
        );
        assert!(!outcome.verdict_approved, "a planted defect is rejected");
        assert!(outcome.verdict_correct);
    }

    #[test]
    fn lens_tier_scores_identically_serial_or_parallel() {
        // The adversary and adjudicator run exactly as before (sequential, after the lens
        // tier); only the lens loop's scheduling changes. lens_workers=1 is the serial walk
        // map_ordered itself runs inline at width one - the oracle the parallel width is
        // compared against.
        let lenses = ["lens-a", "lens-b", "lens-c", "lens-d"];
        let mut ids: Vec<&str> = lenses.to_vec();
        ids.extend(["adv", "adj"]);
        let c = cfg_for(&ids);
        let p = panel_with_lenses(&lenses);
        let it = item("det", "resource-leak", true, "reject", "lens");
        let driver = Scripted {
            catching_tier: TIER_LENS,
            planted_anchors: vec!["det.rs".into()],
            adjudicator_order_sensitive: false,
        };

        let serial = score_item(&driver, &c, &p, &it, 1).unwrap();
        let parallel = score_item(&driver, &c, &p, &it, lenses.len()).unwrap();
        assert_eq!(
            serial, parallel,
            "the parallel lens fan-out scores identically to the serial order"
        );
    }

    #[test]
    fn score_item_counts_findings_raised_per_tier_regardless_of_catch() {
        // Three lenses and the adversary each raise exactly one finding (the Scripted
        // driver always emits one, critical or benign); nobody catches (the anchor is not
        // in planted_anchors), so caught_by is empty while the raw VOLUME is still counted
        // per tier - the two measures are independent.
        let lenses = ["lens-a", "lens-b", "lens-c"];
        let mut ids: Vec<&str> = lenses.to_vec();
        ids.extend(["adv", "adj"]);
        let c = cfg_for(&ids);
        let p = panel_with_lenses(&lenses);
        let it = item("noisy", "off-by-one", true, "reject", "lens");
        let driver = Scripted {
            catching_tier: "", // nobody catches - every finding raised is a benign nit
            planted_anchors: vec![],
            adjudicator_order_sensitive: false,
        };

        let outcome = score_item(&driver, &c, &p, &it, lenses.len()).unwrap();
        assert!(
            outcome.caught_by.is_empty(),
            "nobody caught this (unplanted-for-this-driver) anchor"
        );
        assert_eq!(
            outcome.findings_raised.get(TIER_LENS),
            Some(&3),
            "all three lenses each raised exactly one finding"
        );
        assert_eq!(
            outcome.findings_raised.get(TIER_ADVERSARY),
            Some(&1),
            "the adversary raised its one finding too"
        );
    }

    #[test]
    fn score_item_seeds_a_zero_finding_count_for_a_tier_that_never_ran() {
        // No adversary declared on the panel - the adversary tier never spawns, so its
        // finding count must read an honest 0, not an absent key (mirrors project_canary's
        // own tier-seeding discipline for tier_catch).
        let mut p = panel();
        p.adversary = String::new();
        let driver = Scripted {
            catching_tier: "",
            planted_anchors: vec![],
            adjudicator_order_sensitive: false,
        };
        let it = item("quiet", "off-by-one", true, "reject", "lens");
        let outcome = score_item(&driver, &cfg(), &p, &it, 1).unwrap();
        assert_eq!(
            outcome.findings_raised.get(TIER_ADVERSARY),
            Some(&0),
            "a tier the panel never runs still reports a measured 0, not an absent key"
        );
        assert_eq!(outcome.findings_raised.get(TIER_LENS), Some(&1));
    }

    #[test]
    fn canary_outcome_round_trips_findings_raised_through_the_wire_event() {
        let mut findings_raised = BTreeMap::new();
        findings_raised.insert(TIER_LENS.to_string(), 4);
        findings_raised.insert(TIER_ADVERSARY.to_string(), 2);
        let outcome = CanaryOutcome {
            id: "x".into(),
            defect_class: "off-by-one".into(),
            planted: true,
            expected_reject: true,
            expected_tier: "lens".into(),
            caught_by: vec![TIER_LENS.into()],
            verdict_approved: false,
            verdict_correct: true,
            stable: true,
            findings_raised,
        };
        let event = outcome.to_event("batch");
        let decoded = CanaryOutcome::from_event(&event).expect("a canary outcome decodes back");
        assert_eq!(
            decoded, outcome,
            "findings_raised must round-trip byte-for-byte through the wire event, exactly \
             like every other CanaryOutcome field"
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
        let report = run_canary(&store, &driver, &cfg(), &panel(), &corpus, 2, &|_, _| {}).unwrap();
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
        assert!(run_canary(&store, &driver, &cfg(), &p, &corpus, 1, &|_, _| {}).is_err());
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
                findings_raised: BTreeMap::new(),
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
    #[test]
    fn default_jobs_is_always_greater_than_one() {
        // Spec 61 (ITEM SHARDING AND THE JOBS CAP) requires the --jobs default to be
        // greater than 1. default_jobs floors the crate-wide default worker width at 2 so
        // this holds on every host, including a single-core one where default_workers()
        // itself returns 1 - the assertion is on the FORMULA, not the live host's core
        // count, so it can never be host-flaky.
        assert!(
            default_jobs() > 1,
            "the canary --jobs default must always be greater than one"
        );
    }

    #[test]
    fn spawn_budget_never_lets_the_two_dimensions_product_exceed_jobs() {
        // c5 owns the --jobs total-concurrent-spawn cap spanning BOTH item sharding (this
        // function's item_workers) and the LENS FAN-OUT criterion's inner fan-out
        // (lens_workers) together: the two widths multiplied is the worst-case concurrent
        // spawn count, and it must never exceed the requested budget, for every (jobs,
        // corpus_len) pair including the degenerate zero cases.
        for (jobs, corpus_len) in [
            (0, 0),
            (0, 5),
            (1, 1),
            (1, 10),
            (6, 3),
            (4, 10),
            (10, 4),
            (100, 1),
            (7, 7),
            (usize::MAX, 3),
        ] {
            let (item_workers, lens_workers) = spawn_budget(jobs, corpus_len);
            assert!(item_workers >= 1, "item_workers is never zero");
            assert!(lens_workers >= 1, "lens_workers is never zero");
            assert!(
                item_workers <= corpus_len.max(1),
                "item_workers ({item_workers}) must not exceed the corpus size ({corpus_len}) \
                 - a --jobs value larger than the corpus must not spawn idle item threads"
            );
            assert!(
                item_workers.saturating_mul(lens_workers) <= jobs.max(1),
                "item_workers ({item_workers}) * lens_workers ({lens_workers}) must not \
                 exceed jobs.max(1) ({}) for jobs={jobs}, corpus_len={corpus_len}",
                jobs.max(1)
            );
        }
    }

    #[test]
    fn spawn_budget_uses_the_whole_item_dimension_when_the_corpus_is_the_bottleneck() {
        // A generous --jobs budget against a small corpus should shard every item
        // concurrently (item_workers == corpus_len), not leave items queued while jobs
        // capacity goes unused.
        let (item_workers, lens_workers) = spawn_budget(20, 4);
        assert_eq!(item_workers, 4, "every item shards concurrently");
        assert_eq!(
            lens_workers, 5,
            "the remaining budget funds lens fan-out: 20 / 4 = 5"
        );
    }

    /// A driver that blocks EVERY spawn it receives on a shared barrier before delegating
    /// to a real `Scripted` driver's scoring logic - proving TOTAL concurrent spawns
    /// (summed across every in-flight item's lens fan-out together) reach the barrier's
    /// size, i.e. that item sharding and lens fan-out are genuinely COMBINED, not each
    /// bounded independently by the same `jobs` number twice over.
    struct BarrierGatedEverySpawn {
        barrier: std::sync::Barrier,
        inner: Scripted,
    }

    impl AgentDriver for BarrierGatedEverySpawn {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id != "adv" && a.id != "adj" {
                self.barrier.wait();
            }
            self.inner.spawn(a, prompt, opts, emit)
        }
    }

    #[test]
    fn run_canary_shards_independent_items_concurrently_at_the_scheduling_seam() {
        // Three items, one lens each, jobs=3 so spawn_budget picks item_workers=3,
        // lens_workers=1: three items must have their (sole) lens spawn in flight
        // simultaneously or this deadlocks, proving run_canary's OUTER per-item loop now
        // shards independent items concurrently (not just the inner lens loop c4 built).
        let ids = ["lens-a", "adv", "adj"];
        let c = cfg_for(&ids);
        let p = panel_with_lenses(&["lens-a"]);
        let corpus = vec![
            item("i1", "off-by-one", true, "reject", "lens"),
            item("i2", "off-by-one", true, "reject", "lens"),
            item("i3", "off-by-one", true, "reject", "lens"),
        ];
        let driver = BarrierGatedEverySpawn {
            barrier: std::sync::Barrier::new(3),
            inner: Scripted {
                catching_tier: TIER_LENS,
                planted_anchors: vec!["i1.rs".into(), "i2.rs".into(), "i3.rs".into()],
                adjudicator_order_sensitive: false,
            },
        };
        let store = Store::open(":memory:").unwrap();
        let report = run_canary(&store, &driver, &c, &p, &corpus, 3, &|_, _| {}).unwrap();
        assert_eq!(report.outcomes.len(), 3);
        for outcome in &report.outcomes {
            assert_eq!(outcome.caught_by, vec![TIER_LENS.to_string()]);
            assert!(outcome.verdict_correct);
        }
    }

    #[test]
    fn run_canary_jobs_cap_bounds_total_concurrent_spawns_across_both_dimensions() {
        // Three items x two lenses, jobs=6 (an exact fit: spawn_budget(6,3) = (3,2)).
        // Every lens spawn across the WHOLE run - all items together - blocks on a
        // barrier sized to exactly 3*2=6. This can only pass if item sharding (3
        // concurrent items) and lens fan-out (2 concurrent lenses per item) are truly
        // COMBINED at the same instant: fewer than 6 simultaneous lens spawns anywhere in
        // the system (e.g. items serialized, or lenses serialized within an item) hangs.
        let lenses = ["lens-a", "lens-b"];
        let mut ids: Vec<&str> = lenses.to_vec();
        ids.extend(["adv", "adj"]);
        let c = cfg_for(&ids);
        let p = panel_with_lenses(&lenses);
        let corpus = vec![
            item("i1", "off-by-one", true, "reject", "lens"),
            item("i2", "off-by-one", true, "reject", "lens"),
            item("i3", "off-by-one", true, "reject", "lens"),
        ];
        let driver = BarrierGatedEverySpawn {
            barrier: std::sync::Barrier::new(6),
            inner: Scripted {
                catching_tier: TIER_LENS,
                planted_anchors: vec!["i1.rs".into(), "i2.rs".into(), "i3.rs".into()],
                adjudicator_order_sensitive: false,
            },
        };
        let store = Store::open(":memory:").unwrap();
        let report = run_canary(&store, &driver, &c, &p, &corpus, 6, &|_, _| {}).unwrap();
        assert_eq!(report.outcomes.len(), 3);
        for outcome in &report.outcomes {
            assert_eq!(outcome.caught_by, vec![TIER_LENS.to_string()]);
            assert!(outcome.verdict_correct);
        }
    }

    #[test]
    fn run_canary_scores_identically_regardless_of_the_jobs_width() {
        // The determinism constraint: sharding items and fanning lenses out must not
        // change any scored outcome. jobs=1 forces the fully serial walk on both
        // dimensions (spawn_budget(1, n) = (1,1)); a generous jobs engages real
        // concurrency on both dimensions. The aggregated report must be byte-identical,
        // in the SAME item order, either way.
        let lenses = ["lens-a", "lens-b", "lens-c"];
        let mut ids: Vec<&str> = lenses.to_vec();
        ids.extend(["adv", "adj"]);
        let c = cfg_for(&ids);
        let p = panel_with_lenses(&lenses);
        let corpus = vec![
            item("i1", "off-by-one", true, "reject", "lens"),
            item("i2", "resource-leak", true, "reject", "adversary"),
            item("i3", "none", false, "approve", ""),
            item("i4", "off-by-one", true, "reject", "lens"),
        ];
        let driver = Scripted {
            catching_tier: TIER_LENS,
            planted_anchors: vec!["i1.rs".into(), "i4.rs".into()],
            adjudicator_order_sensitive: false,
        };

        let serial_store = Store::open(":memory:").unwrap();
        let serial_progress: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        let serial = run_canary(&serial_store, &driver, &c, &p, &corpus, 1, &|o, _| {
            serial_progress.lock().unwrap().push(o.id.clone());
        })
        .unwrap();
        let parallel_store = Store::open(":memory:").unwrap();
        let parallel_progress: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        let parallel = run_canary(&parallel_store, &driver, &c, &p, &corpus, 12, &|o, _| {
            parallel_progress.lock().unwrap().push(o.id.clone());
        })
        .unwrap();

        assert_eq!(
            serial.outcomes, parallel.outcomes,
            "sharded/fanned-out scoring must match the serial walk exactly, in order"
        );
        // The on_item hook must fire exactly once per item at BOTH widths - not zero
        // (dropped), not doubled (a stray second call site) - regardless of whether
        // map_ordered took its serial or its threaded path.
        let mut serial_ids = serial_progress.into_inner().unwrap();
        let mut parallel_ids = parallel_progress.into_inner().unwrap();
        serial_ids.sort();
        parallel_ids.sort();
        let mut expected: Vec<String> = corpus.iter().map(|i| i.id.clone()).collect();
        expected.sort();
        assert_eq!(
            serial_ids, expected,
            "the serial (jobs=1) walk must call on_item exactly once per item"
        );
        assert_eq!(
            parallel_ids, expected,
            "the sharded (jobs=12) walk must call on_item exactly once per item"
        );
    }

    /// A driver that blocks any non-adjudicator spawn naming `slow.rs` on a shared
    /// gate before delegating to a real `Scripted` driver - the synchronization
    /// primitive [`on_item_fires_the_instant_a_fast_items_score_completes_while_a_slower_sibling_is_still_scoring`]
    /// uses to prove the progress hook streams rather than bunches.
    struct GateSlowOnFastsProgress {
        released: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
        inner: Scripted,
    }

    impl AgentDriver for GateSlowOnFastsProgress {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            if a.id != "adj" && prompt.contains("`slow.rs`") {
                let (lock, cvar) = &*self.released;
                let guard = lock.lock().unwrap();
                let (_guard, result) = cvar
                    .wait_timeout_while(guard, Duration::from_secs(2), |released| !*released)
                    .unwrap();
                assert!(
                    !result.timed_out(),
                    "the slow item's spawn ran without ever observing the fast item's \
                     on_item progress callback fire - the hook is bunched at the \
                     aggregation loop (after map_ordered joins every worker), not \
                     streamed per item as each one genuinely completes"
                );
            }
            self.inner.spawn(a, prompt, opts, emit)
        }
    }

    #[test]
    fn on_item_fires_the_instant_a_fast_items_score_completes_while_a_slower_sibling_is_still_scoring(
    ) {
        // Two items sharded onto two threads (item_workers=2 at jobs=2, corpus len 2).
        // "fast" has nothing gating it and scores immediately. EVERY one of "slow"'s own
        // spawns blocks until on_item has already fired for "fast" - so "slow"'s
        // score_item provably cannot return before that callback ran. If the hook were
        // instead placed after map_ordered joins ALL workers (the exact regression this
        // criterion exists to close - see the sdet finding this closes,
        // sdet-u61c5-progress-hook-is-bunched-not-streaming), on_item(fast) could never
        // fire before "slow" needs it (both threads are still inside map_ordered at that
        // point), so the wait below would exhaust its bound and fail loudly instead of
        // silently passing.
        let released =
            std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let ids = ["lens-a", "adv", "adj"];
        let c = cfg_for(&ids);
        let p = panel_with_lenses(&["lens-a"]);
        let corpus = vec![
            item("fast", "off-by-one", true, "reject", "lens"),
            item("slow", "off-by-one", true, "reject", "lens"),
        ];
        let driver = GateSlowOnFastsProgress {
            released: released.clone(),
            inner: Scripted {
                catching_tier: TIER_LENS,
                planted_anchors: vec!["fast.rs".into(), "slow.rs".into()],
                adjudicator_order_sensitive: false,
            },
        };
        let store = Store::open(":memory:").unwrap();
        let report = run_canary(&store, &driver, &c, &p, &corpus, 2, &|o, _| {
            if o.id == "fast" {
                let (lock, cvar) = &*released;
                *lock.lock().unwrap() = true;
                cvar.notify_all();
            }
        })
        .unwrap();
        assert_eq!(
            report.outcomes.len(),
            2,
            "both items still score to completion"
        );
    }

    #[test]
    fn on_item_receives_the_same_outcome_content_it_records_and_a_real_elapsed_duration() {
        // A single item whose lens spawn sleeps a known amount mid-score: the elapsed
        // duration on_item receives must reflect that real wall-clock time, not a
        // zero/fixed placeholder - and the (id, verdict_correct, caught_by) it carries
        // must be the SAME values the returned report records, not a second,
        // independently-derived copy that could silently drift from it.
        struct SleepsOnTheLensSpawn {
            inner: Scripted,
        }
        impl AgentDriver for SleepsOnTheLensSpawn {
            fn spawn(
                &self,
                a: &AgentDef,
                prompt: &str,
                opts: &SpawnOpts,
                emit: &dyn Fn(&str, Value) -> Result<(), Error>,
            ) -> Result<AgentResult, Error> {
                if a.id == "lens-a" {
                    std::thread::sleep(Duration::from_millis(30));
                }
                self.inner.spawn(a, prompt, opts, emit)
            }
        }

        let ids = ["lens-a", "adv", "adj"];
        let c = cfg_for(&ids);
        let p = panel_with_lenses(&["lens-a"]);
        let corpus = vec![item("hot", "off-by-one", true, "reject", "lens")];
        let driver = SleepsOnTheLensSpawn {
            inner: Scripted {
                catching_tier: TIER_LENS,
                planted_anchors: vec!["hot.rs".into()],
                adjudicator_order_sensitive: false,
            },
        };
        // (id, verdict_correct, caught_by, elapsed) - what on_item is asked to carry.
        type Seen = (String, bool, Vec<String>, Duration);

        let store = Store::open(":memory:").unwrap();
        let seen: std::sync::Mutex<Vec<Seen>> = std::sync::Mutex::new(Vec::new());
        let report = run_canary(&store, &driver, &c, &p, &corpus, 1, &|o, elapsed| {
            seen.lock().unwrap().push((
                o.id.clone(),
                o.verdict_correct,
                o.caught_by.clone(),
                elapsed,
            ));
        })
        .unwrap();

        let seen = seen.into_inner().unwrap();
        assert_eq!(seen.len(), 1, "on_item fires exactly once for the one item");
        let (id, correct, caught, elapsed) = &seen[0];
        let recorded = &report.outcomes[0];
        assert_eq!(id, &recorded.id);
        assert_eq!(*correct, recorded.verdict_correct);
        assert_eq!(caught, &recorded.caught_by);
        assert!(
            *elapsed >= Duration::from_millis(25),
            "elapsed must be the item's real measured scoring duration, not a zero/fake \
             value - the driver slept 30ms mid-score; got {elapsed:?}"
        );
    }

    /// A CANARY SCORE THE STORE DID NOT WRITE IS NOT A SCORE. The batch marker and every
    /// per-item outcome are the ONLY durable record the canary produces - the returned
    /// report is for the CLI's summary print and is gone at process exit - so a write this
    /// seam loses is a measurement that reads as taken and cannot be found afterwards.
    ///
    /// The append used to discard its report with `?;`, which kept compiling when the port
    /// stopped promising a position. It now asks the same authority every other
    /// single-event seam asks, so the loss surfaces where it happened.
    #[test]
    fn a_canary_record_the_store_did_not_write_fails_rather_than_scoring_on() {
        let event = Event::new(TYPE_UNIT_STATUS, b"{}".to_vec());
        let err = append(&crate::eventstore::SilentStore, event)
            .expect_err("a scored outcome nobody can find was not recorded");
        let message = err.to_string();
        assert!(
            message.contains("nothing"),
            "the failure says the store wrote nothing: {message}"
        );
        assert!(
            message.contains(TYPE_UNIT_STATUS),
            "and names the record that was lost: {message}"
        );
    }
}
