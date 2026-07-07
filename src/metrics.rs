//! Operator metrics, projected from the conductor's event log.
//!
//! This is a third CQRS read-model over the one append-only event stream, beside
//! [`crate::ledger`] (durable run state) and [`crate::contextgraph`] (the
//! knowledge graph). It answers a distinct question - *is the implement -> review
//! loop earning its cost?* - by folding an ordered `&[Event]` slice into a
//! [`Metrics`] summary: first-pass yield, per-gate remediation counts, escalation
//! rate, and review approve/reject counts.
//!
//! Like the other projections it is a pure replay: [`project`] mirrors
//! [`crate::ledger::project`], applies each event in order, and ignores unknown or
//! malformed events so the same shared log can feed all three read-models. It
//! single-sources the event-type *vocabulary* (it imports [`ledger::TYPE_*`] and
//! [`contextgraph::TYPE_GATE_VERDICT`] rather than re-declaring them); the wire
//! *payload* structs in those modules are private, so the few fields this fold
//! needs are decoded locally with [`field_str`] / [`gate_verdict`].
//!
//! # Review approve/reject classification
//!
//! A unit's review outcome is read off the conductor's two review paths:
//!
//! - **Per-unit review** (`run_single_stage`): the conductor emits a `verified`
//!   `UnitStatus` once the gates pass, *then* runs the adjudicator. An approve
//!   integrates; a reject loops back into `UnitFailed` with no intervening
//!   `reviewed`. So a per-unit reject is detected by `verified`-then-`UnitFailed`,
//!   and a gate failure (which never emits `verified`) is correctly not counted.
//! - **Fan-out / standalone review** (`run_fan_out_review_loop`): this path emits
//!   `reviewed` on approve but only a bare `UnitFailed` on reject, with **no
//!   `verified`**. There is no per-unit cause discriminator on `UnitFailed`, so a
//!   fan-out reject is inferred from the unit's `UnitStarted` carrying an **empty
//!   `agent`** (the necessary condition for `is_fan_out`).
//!
//! The empty-`agent` signal is a **lossy heuristic, not an exact classifier**: an
//! empty agent is *necessary* for a fan-out / review-only stage but not
//! *sufficient*. A non-fan-out stage may also be authored with an empty `agent`
//! (config validation never requires one) and run real code gates; its bare
//! gate-failure `UnitFailed` then has the same shape as a review reject and is
//! counted as one. This **known, accepted false positive** is the price of the
//! conductor not stamping a cause on `UnitFailed`; an exact split would need a
//! `cause` field on the producer, which is out of this read-model's scope. The
//! regression test `agentless_gate_failure_is_counted_as_a_review_reject_known_false_positive`
//! pins the trade-off. The approve count (`reviewed`) is exact.
//!
//! Reject counts are reported in **aggregate only**. The spec allows splitting
//! rejects by flagging tier *where the actor metadata makes it derivable*, but the
//! conductor stamps no `META_ACTOR` on either `UnitFailed` emit site (both route
//! through `emit_with_actor("")`), so a per-tier split would be permanently empty
//! on any real log - the spec's "otherwise report the aggregate" applies.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

use crate::conductor::META_WORKTREE_SHA;
use crate::contextgraph::{META_ACTOR, TYPE_GATE_VERDICT, TYPE_REVIEW_FINDING};
use crate::eventstore::Event;
use crate::ledger::{
    TYPE_UNIT_ESCALATED, TYPE_UNIT_FAILED, TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED,
    TYPE_UNIT_STATUS,
};
use crate::spawn::{SpawnResult, ROLE_ADJUDICATOR, ROLE_ADVERSARY, TYPE_SPAWN_RESULT};

/// The tier a review spawn belongs to, recovered from its deterministic
/// [`spawn_id`](crate::spawn::spawn_id) `{unit}/{role}#{attempt}` (a retry id may add a
/// `~retryN` suffix, which is trimmed here). Returns the tier label the fold keys cost
/// under - `"lens"`, `"adversary"`, `"adjudicator"` - or `None` for a non-review spawn
/// (the implementer). The `lens:` role prefix mirrors [`crate::spawn::lens_role`].
fn review_tier(spawn_id: &str) -> Option<&'static str> {
    let role = spawn_id
        .rsplit_once('/')
        .map(|(_, r)| r)
        .unwrap_or(spawn_id);
    let role = role.split(['#', '~']).next().unwrap_or(role);
    if role == ROLE_ADVERSARY {
        Some("adversary")
    } else if role == ROLE_ADJUDICATOR {
        Some("adjudicator")
    } else if role.starts_with("lens:") {
        Some("lens")
    } else {
        None
    }
}

/// The tier a finding's ACTOR belongs to for cost/precision accounting: the adversary
/// role token is the adversary tier; every other finding-raising actor is a lens (the
/// adjudicator raises no findings). Single-sources the lens-vs-adversary split the
/// per-tier cost and adversary-precision folds share.
fn actor_tier(actor: &str) -> &'static str {
    if actor == ROLE_ADVERSARY {
        "adversary"
    } else {
        "lens"
    }
}

/// Pass/fail tallies for one gate id across a run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateCounts {
    /// `GateVerdict` events with `pass:true` (artifact-tagged ones excluded).
    pub pass: u64,
    /// `GateVerdict` events with `pass:false` - the remediation signal.
    pub fail: u64,
}

impl GateCounts {
    /// Total real gate runs recorded for this gate.
    pub fn total(&self) -> u64 {
        self.pass + self.fail
    }
}

/// The operator-facing metrics for one run, folded from the event log.
///
/// Percentages (`first_pass_yield`, `escalation_rate`) are derived on demand
/// rather than stored, so the struct stays a plain count aggregate and can derive
/// [`Eq`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    /// Distinct units that emitted a `UnitStarted` (deduplicated across resumes).
    pub units_started: u64,
    /// Units that reached `Integrated` with zero `UnitFailed` for their id - a
    /// clean first pass. The numerator of first-pass yield.
    pub first_pass_clean: u64,
    /// Per-gate pass/fail tallies, excluding the artifact-tagged integrate-time
    /// `GateVerdict`s. Sorted by gate id (`BTreeMap`) for stable reporting.
    pub gates: BTreeMap<String, GateCounts>,
    /// Distinct units that emitted `UnitEscalated`. The numerator of the
    /// escalation rate.
    pub units_escalated: u64,
    /// Reviews that approved: a `reviewed` `UnitStatus` (exact count).
    pub review_approve: u64,
    /// Reviews that rejected and looped back into `UnitFailed` without
    /// integrating on that attempt. Aggregate only.
    ///
    /// Detected on both conductor review paths: the per-unit path arms on a
    /// `verified` `UnitStatus` then counts a `UnitFailed`-while-armed; the
    /// fan-out / review-only path is inferred from an empty-`agent` `UnitStarted`
    /// then a `UnitFailed`. The fan-out signal is **lossy** - see the module doc
    /// for the known/accepted false positive (an agentless gated stage's bare gate
    /// failure).
    pub review_reject: u64,
    /// The spec-11 unit-1 review-QUALITY telemetry (is the review tier RIGHT, not just
    /// how often it rejects), folded from the sha-stamped review-boundary events, the
    /// `ReviewFinding`s the tiers raise, and the adjudicator `SpawnResult` verdicts.
    pub review_quality: ReviewQuality,
}

/// Raised-vs-upheld tally for one review actor's (or tier's) findings. `survival` is the
/// fraction the adjudicator upheld - the review-quality signal the aggregate reject
/// count cannot see.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindingCounts {
    /// Findings this actor RAISED (distinct `ReviewFinding` ids attributed to it).
    pub raised: u64,
    /// Of those, how many an adjudicator listed in its `upheld` verdict field.
    pub upheld: u64,
}

impl FindingCounts {
    /// The fraction of this actor's raised findings the adjudicator upheld, in
    /// `[0.0, 1.0]`; `0.0` when it raised none (never `NaN`).
    pub fn survival(&self) -> f64 {
        ratio(self.upheld, self.raised)
    }
}

/// Per-tier review cost proxy: how many review spawns that tier ran against how many of
/// its findings survived adjudication. A spawn-count proxy (not a dollar figure): the
/// Gap-6 model stamps that would price each spawn ride the same events, but weighting
/// them by a price table is Wave-4 acting-on-measurements, out of this read-model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TierCost {
    /// `SpawnResult`s recorded for this tier's role (the cost incurred).
    pub spawns: u64,
    /// Findings this tier raised that were upheld (the value delivered).
    pub upheld: u64,
}

impl TierCost {
    /// Review spawns per upheld finding for this tier - lower is cheaper review. `0.0`
    /// when the tier upheld nothing (the ratio is undefined; render it as "not yet
    /// earning" rather than a division by zero).
    pub fn cost_per_upheld(&self) -> f64 {
        if self.upheld == 0 {
            0.0
        } else {
            self.spawns as f64 / self.upheld as f64
        }
    }
}

/// Review-quality telemetry (spec 11, unit 1 - "the cheap six"): measures whether the
/// review tier is RIGHT, folded from the same run stream as the rest of [`Metrics`].
/// Every field is an integer count so [`Metrics`] keeps deriving [`Eq`]; the rates are
/// derived on demand.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReviewQuality {
    /// Review REJECTIONS later APPROVED on the SAME worktree sha (spec 11): the tiers
    /// flipped their verdict on unchanged code, so this is reviewer noise. Paired via the
    /// [`META_WORKTREE_SHA`] stamp on the review-reject `UnitFailed` and the `reviewed`.
    pub flip_flops: u64,
    /// Per-actor finding survival (upheld / raised), keyed by the conductor-stamped
    /// [`META_ACTOR`] (falling back to the finding's `by`), sorted for stable reporting.
    pub finding_survival: BTreeMap<String, FindingCounts>,
    /// Files carrying findings from TWO OR MORE distinct actors - the same defect flagged
    /// by more than one lens. The numerator of the lens-overlap rate (retroactive over
    /// all history).
    pub overlap_files: u64,
    /// Files carrying at least one finding - the denominator of the lens-overlap rate.
    pub finding_files: u64,
    /// Review rejections split by the adjudicator-declared `cause`
    /// (`genuine-defect` / `spec-ambiguity` / `decomposition-conflict` / `infra-fault`),
    /// sorted by cause.
    pub rejections_by_cause: BTreeMap<String, u64>,
    /// Escalations split by the `cause` of the escalating unit's FINAL rejection.
    pub escalations_by_cause: BTreeMap<String, u64>,
    /// Adversary-ONLY findings (raised by the adversary about files no lens also flagged)
    /// upheld vs raised - the adversary's unique catch rate, its precision.
    pub adversary_only: FindingCounts,
    /// Per-tier cost proxy (`"lens"` / `"adversary"` / `"adjudicator"`): review spawns vs
    /// upheld findings, sorted by tier.
    pub tier_cost: BTreeMap<String, TierCost>,
}

impl ReviewQuality {
    /// The lens-overlap rate in `[0.0, 1.0]`: files flagged by two or more actors over
    /// files flagged at all. `0.0` when no findings touched any file.
    pub fn lens_overlap_rate(&self) -> f64 {
        ratio(self.overlap_files, self.finding_files)
    }

    /// Adversary precision in `[0.0, 1.0]`: adversary-only findings upheld over
    /// adversary-only findings raised. `0.0` when the adversary raised no unique finding.
    pub fn adversary_precision(&self) -> f64 {
        self.adversary_only.survival()
    }
}

impl Metrics {
    /// First-pass yield as a fraction in `[0.0, 1.0]`: clean first passes over
    /// total units started. Zero when no units started (no division by zero).
    pub fn first_pass_yield(&self) -> f64 {
        ratio(self.first_pass_clean, self.units_started)
    }

    /// Escalation rate as a fraction in `[0.0, 1.0]`: escalated units over total
    /// units started. Zero when no units started.
    pub fn escalation_rate(&self) -> f64 {
        ratio(self.units_escalated, self.units_started)
    }

    /// Rejection flip-flop rate in `[0.0, 1.0]`: rejections later approved on the SAME
    /// worktree sha over all review rejections. Zero when nothing was rejected. This is
    /// the share of rejects that were reviewer noise (a verdict flip on unchanged code).
    pub fn flip_flop_rate(&self) -> f64 {
        ratio(self.review_quality.flip_flops, self.review_reject)
    }
}

/// A guarded fraction: `num / den`, or `0.0` when `den == 0` (never `NaN`).
fn ratio(num: u64, den: u64) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

/// Per-unit fold state, keyed by unit id so concurrently-interleaved units in the
/// append-only stream never bleed into each other.
#[derive(Default)]
struct UnitFold {
    /// Saw a real `UnitStarted` (the dedup + numerator-subset guard).
    started: bool,
    /// The unit's `UnitStarted` carried an empty `agent` - the necessary (lossy)
    /// signal for a fan-out / review-only stage whose reject is a bare
    /// `UnitFailed`.
    review_only: bool,
    /// A `verified` `UnitStatus` is in flight (per-unit review armed): the next
    /// `UnitFailed` for this id is a review reject, not a gate failure.
    armed: bool,
    /// This unit emitted at least one `UnitFailed`, so it cannot be a clean first
    /// pass even if it later integrates.
    failed: bool,
    /// This unit reached `Integrated`.
    integrated: bool,
    /// This unit's escalation has already been tallied (idempotency guard against
    /// a malformed double-emit; the conductor emits `UnitEscalated` once per id).
    escalated: bool,
    /// Worktree shas this unit was review-REJECTED on. A later `reviewed` (approve)
    /// on any sha in this set is a flip-flop (spec 11): a verdict reversal on the same
    /// code the tiers already rejected.
    rejected_shas: BTreeSet<String>,
    /// The adjudicator-declared `cause` of this unit's most recent review rejection,
    /// carried so a subsequent `UnitEscalated` can attribute the escalation to it.
    last_reject_cause: Option<String>,
}

/// Fold an ordered event slice into the operator [`Metrics`], mirroring
/// [`crate::ledger::project`]. Pure and replay-safe: unknown event types and
/// malformed payloads are ignored, so the same shared log feeds this read-model
/// alongside the ledger and the context graph.
pub fn project(events: &[Event]) -> Metrics {
    let mut units: BTreeMap<String, UnitFold> = BTreeMap::new();
    let mut metrics = Metrics::default();

    // Review-quality accumulators (spec 11, unit 1), resolved into `metrics.review_quality`
    // after the pass. Findings are folded as they are raised; the adjudicator verdicts that
    // uphold them and stamp a rejection `cause` arrive later in the same forward stream.
    // finding id -> the actor that raised it (empty when unattributed).
    let mut finding_actor: BTreeMap<String, String> = BTreeMap::new();
    // finding id -> the files it concerns (for adversary-only attribution).
    let mut finding_about: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // file -> the distinct non-empty actors that raised a finding about it (lens overlap).
    let mut file_actors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // finding ids an adjudicator upheld (union across all verdicts).
    let mut upheld: BTreeSet<String> = BTreeSet::new();
    // review rejections split by adjudicator-declared cause.
    let mut rejections_by_cause: BTreeMap<String, u64> = BTreeMap::new();
    // per-unit rejection cause stashed by the adjudicator verdict, consumed by the
    // matching review-reject `UnitFailed` (the verdict precedes the failure in-stream).
    let mut pending_cause: BTreeMap<String, String> = BTreeMap::new();
    // review spawns recorded per tier (the cost side of cost-per-upheld).
    let mut tier_spawns: BTreeMap<String, u64> = BTreeMap::new();

    for e in events {
        match e.type_.as_str() {
            TYPE_UNIT_STARTED => {
                // The conductor emits UnitStarted with `id` and `agent`; on resume
                // it re-emits for a not-yet-integrated unit, so dedup on first sight
                // keeps units_started (the yield / escalation denominator) honest.
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                let u = units.entry(id).or_default();
                if !u.started {
                    u.started = true;
                    // Empty agent is the necessary (lossy) marker of a fan-out /
                    // review-only stage; see the module doc for the false positive.
                    u.review_only = field_str(e, "agent").unwrap_or_default().is_empty();
                    metrics.units_started += 1;
                }
            }
            TYPE_UNIT_STATUS => {
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                let Some(status) = field_str(e, "status") else {
                    continue;
                };
                let u = units.entry(id).or_default();
                match status.as_str() {
                    // Per-unit review: `verified` means gates passed and the
                    // adjudicator is about to run; arm the reject detector so a
                    // following UnitFailed is read as a review reject (a gate
                    // failure never reaches `verified`).
                    "verified" => u.armed = true,
                    // A `reviewed` status is the standalone/fan-out approve signal,
                    // exact. Gate the increment on a started unit so the count stays
                    // a subset of units_started.
                    "reviewed" if u.started => {
                        metrics.review_approve += 1;
                        // Flip-flop (spec 11): an approve on a sha this unit was already
                        // REJECTED on is a verdict reversal over unchanged code - reviewer
                        // noise. Matched by the worktree-sha meta stamped on both events.
                        if let Some(sha) = e.meta.get(META_WORKTREE_SHA).filter(|s| !s.is_empty()) {
                            if u.rejected_shas.contains(sha) {
                                metrics.review_quality.flip_flops += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
            TYPE_UNIT_FAILED => {
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                let u = units.entry(id.clone()).or_default();
                u.failed = true;
                // Classify the failure as a review reject on either review path,
                // gated on a started unit so review_reject stays a subset of
                // units_started:
                //   - per-unit: a `verified` status armed the reject;
                //   - fan-out / review-only: the lossy empty-`agent` heuristic (see
                //     the module doc for the accepted false positive).
                // A gate / spawn failure of a normal, agent-named unit matches
                // neither arm and is correctly not counted.
                if u.started && (u.armed || u.review_only) {
                    metrics.review_reject += 1;
                    // Record the rejected sha so a later approve on it reads as a
                    // flip-flop (spec 11).
                    if let Some(sha) = e.meta.get(META_WORKTREE_SHA).filter(|s| !s.is_empty()) {
                        u.rejected_shas.insert(sha.clone());
                    }
                    // Attribute the reject to the cause the adjudicator's verdict stashed
                    // for this unit (cause-split rejection rate); carry it so an escalation
                    // of this unit can inherit its final cause.
                    if let Some(cause) = pending_cause.remove(&id) {
                        *rejections_by_cause.entry(cause.clone()).or_default() += 1;
                        u.last_reject_cause = Some(cause);
                    }
                }
                // The reject (or remediation) is consumed; disarm so a later retry's
                // gate failure on the same id is not double-counted.
                u.armed = false;
            }
            TYPE_UNIT_ESCALATED => {
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                let u = units.entry(id).or_default();
                if !u.escalated {
                    u.escalated = true;
                    metrics.units_escalated += 1;
                    // Cause-split escalation rate (spec 11): attribute this escalation to
                    // the cause of the unit's final review rejection, when one was recorded.
                    if let Some(cause) = &u.last_reject_cause {
                        *metrics
                            .review_quality
                            .escalations_by_cause
                            .entry(cause.clone())
                            .or_default() += 1;
                    }
                }
            }
            TYPE_UNIT_INTEGRATED => {
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                units.entry(id).or_default().integrated = true;
            }
            TYPE_GATE_VERDICT => {
                // Decode only the three fields this read-model needs. Artifact-tagged
                // verdicts are the integrate-time GATED_BY bookkeeping (one per
                // changed file), NOT real gate runs, so exclude them - the count
                // must reflect gate noise, not how many files a unit touched.
                let Some(v) = gate_verdict(e) else {
                    continue;
                };
                if !v.artifact.is_empty() {
                    continue;
                }
                let counts = metrics.gates.entry(v.gate).or_default();
                if v.pass {
                    counts.pass += 1;
                } else {
                    counts.fail += 1;
                }
            }
            TYPE_REVIEW_FINDING => {
                // A finding a lens or the adversary raised. Attribute it to the
                // conductor-stamped META_ACTOR (falling back to the payload `by`, exactly
                // as the context-graph fold resolves provenance), and record the files it
                // concerns for the lens-overlap and adversary-only folds. First sighting
                // per id wins (a defensive dedup against a replayed finding).
                let Some(id) = field_str(e, "id") else {
                    continue;
                };
                if finding_actor.contains_key(&id) {
                    continue;
                }
                let actor = e
                    .meta
                    .get(META_ACTOR)
                    .filter(|a| !a.is_empty())
                    .cloned()
                    .or_else(|| field_str(e, "by").filter(|b| !b.is_empty()))
                    .unwrap_or_default();
                let about = field_str_vec(e, "about");
                if !actor.is_empty() {
                    for f in &about {
                        file_actors
                            .entry(f.clone())
                            .or_default()
                            .insert(actor.clone());
                    }
                }
                finding_about.insert(id.clone(), about);
                finding_actor.insert(id, actor);
            }
            TYPE_SPAWN_RESULT => {
                // A recorded review spawn: count it under its tier (cost side), and - for
                // the adjudicator - parse the grown verdict JSON for the findings it upheld
                // and the rejection cause (spec 11's already-durably-logged raw output).
                let Ok(res) = SpawnResult::from_event(e) else {
                    continue;
                };
                let Some(tier) = review_tier(&res.id) else {
                    continue;
                };
                *tier_spawns.entry(tier.to_string()).or_default() += 1;
                if tier == "adjudicator" {
                    if let Some(adj) = parse_adjudication(&res.output) {
                        for fid in adj.upheld {
                            upheld.insert(fid);
                        }
                        // The cause rides only a REJECT verdict (contract); stash it for the
                        // matching review-reject UnitFailed, keyed by this spawn's unit.
                        if let Some(cause) = adj.cause {
                            let unit = res.id.split('/').next().unwrap_or(&res.id).to_string();
                            pending_cause.insert(unit, cause);
                        }
                    }
                }
            }
            // Unknown / foreign event types (DecisionMade, LessonLearned, ...) are
            // ignored so the same shared log feeds every read-model.
            _ => {}
        }
    }

    // ---- Finalize the review-quality folds from the accumulated findings/verdicts ----
    metrics.review_quality.rejections_by_cause = rejections_by_cause;
    // Per-actor finding survival and the adversary's unique-catch precision.
    for (id, actor) in &finding_actor {
        if actor.is_empty() {
            continue;
        }
        let is_upheld = upheld.contains(id);
        let c = metrics
            .review_quality
            .finding_survival
            .entry(actor.clone())
            .or_default();
        c.raised += 1;
        if is_upheld {
            c.upheld += 1;
        }
        // Adversary-ONLY: an adversary finding about files NO lens also flagged.
        if actor == ROLE_ADVERSARY {
            let files = finding_about.get(id).cloned().unwrap_or_default();
            let shared_with_a_lens = files.iter().any(|f| {
                file_actors
                    .get(f)
                    .is_some_and(|acts| acts.iter().any(|a| !a.is_empty() && a != ROLE_ADVERSARY))
            });
            if !shared_with_a_lens {
                metrics.review_quality.adversary_only.raised += 1;
                if is_upheld {
                    metrics.review_quality.adversary_only.upheld += 1;
                }
            }
        }
    }
    // Lens overlap: files flagged by two or more distinct actors, over files flagged at
    // all (retroactive across the whole slice).
    metrics.review_quality.finding_files = file_actors.len() as u64;
    metrics.review_quality.overlap_files =
        file_actors.values().filter(|acts| acts.len() >= 2).count() as u64;
    // Per-tier cost: the spawn count for each tier joined with the upheld findings that
    // tier delivered (lens actors roll up to "lens", the adversary to "adversary").
    let mut tier_cost: BTreeMap<String, TierCost> = BTreeMap::new();
    for (tier, &spawns) in &tier_spawns {
        tier_cost.entry(tier.clone()).or_default().spawns = spawns;
    }
    for (actor, counts) in &metrics.review_quality.finding_survival {
        tier_cost
            .entry(actor_tier(actor).to_string())
            .or_default()
            .upheld += counts.upheld;
    }
    metrics.review_quality.tier_cost = tier_cost;

    // First-pass-clean numerator: integrated with zero failures, gated on a real
    // UnitStarted so the numerator stays a subset of units_started.
    metrics.first_pass_clean = units
        .values()
        .filter(|u| u.started && u.integrated && !u.failed)
        .count() as u64;

    metrics
}

/// The minimal `GateVerdict` view this fold needs. The canonical payload struct in
/// [`crate::contextgraph`] is private, so the three load-bearing fields are decoded
/// locally; the event-*type* constant is still single-sourced from there.
#[derive(Deserialize)]
struct GateVerdictView {
    gate: String,
    #[serde(default)]
    pass: bool,
    #[serde(default)]
    artifact: String,
}

/// Decode a `GateVerdict` payload, or `None` for a malformed one (the sentinel arm
/// that keeps the fold panic-free on a foreign / partial event).
fn gate_verdict(e: &Event) -> Option<GateVerdictView> {
    serde_json::from_slice(&e.data).ok()
}

/// Pull a single string field out of an event's JSON payload, or `None` if the
/// payload is not a JSON object or the field is absent / not a string. This is the
/// sentinel that lets the fold ignore malformed events without panicking.
fn field_str(e: &Event, key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(&e.data).ok()?;
    value.get(key)?.as_str().map(str::to_owned)
}

/// Pull a string-array field out of an event's JSON payload as a `Vec<String>`; empty
/// when the payload is malformed, the field is absent, or it is not an array of strings.
/// The `about` files of a `ReviewFinding` are read this way.
fn field_str_vec(e: &Event, key: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&e.data) else {
        return Vec::new();
    };
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The review-quality fields this fold reads off an adjudicator's grown verdict line.
struct Adjudication {
    /// The finding ids the adjudicator upheld (its `upheld` array).
    upheld: Vec<String>,
    /// The rejection `cause`, present only on a reject verdict (`None` on approve or when
    /// the adjudicator declared none).
    cause: Option<String>,
}

/// Parse an adjudicator's raw output for its grown JSON verdict line (spec 11): the LAST
/// JSON object line carrying a `verdict`, `upheld`, or `discarded` field. Returns the
/// upheld finding ids and the rejection cause, or `None` when no verdict line is present
/// (an old-contract adjudicator, or unparseable output) - the fold then attributes
/// nothing, exactly like the empty per-tier reject split on a log that never stamped it.
fn parse_adjudication(output: &str) -> Option<Adjudication> {
    for line in output.lines().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if v.get("verdict").is_none() && v.get("upheld").is_none() && v.get("discarded").is_none() {
            continue;
        }
        let upheld = v
            .get("upheld")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let cause = v
            .get("cause")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|s| !s.is_empty());
        return Some(Adjudication { upheld, cause });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(type_: &str, data: &str) -> Event {
        Event::new(type_, data.as_bytes().to_vec())
    }

    fn started(id: &str, agent: &str) -> Event {
        ev(
            TYPE_UNIT_STARTED,
            &format!(r#"{{"id":"{id}","agent":"{agent}"}}"#),
        )
    }

    fn status(id: &str, status: &str) -> Event {
        ev(
            TYPE_UNIT_STATUS,
            &format!(r#"{{"id":"{id}","status":"{status}"}}"#),
        )
    }

    fn failed(id: &str) -> Event {
        ev(
            TYPE_UNIT_FAILED,
            &format!(r#"{{"id":"{id}","attempts":1}}"#),
        )
    }

    fn integrated(id: &str) -> Event {
        ev(
            TYPE_UNIT_INTEGRATED,
            &format!(r#"{{"id":"{id}","commit":"abc"}}"#),
        )
    }

    fn escalated(id: &str) -> Event {
        ev(TYPE_UNIT_ESCALATED, &format!(r#"{{"id":"{id}"}}"#))
    }

    fn verdict(gate: &str, pass: bool) -> Event {
        ev(
            TYPE_GATE_VERDICT,
            &format!(r#"{{"gate":"{gate}","pass":{pass}}}"#),
        )
    }

    fn artifact_verdict(gate: &str, artifact: &str) -> Event {
        ev(
            TYPE_GATE_VERDICT,
            &format!(r#"{{"gate":"{gate}","pass":true,"artifact":"{artifact}"}}"#),
        )
    }

    #[test]
    fn empty_log_has_zeroed_metrics_and_no_nan() {
        let m = project(&[]);
        assert_eq!(m, Metrics::default());
        assert_eq!(m.first_pass_yield(), 0.0);
        assert_eq!(m.escalation_rate(), 0.0);
    }

    #[test]
    fn projects_every_metric_from_a_synthetic_run() {
        // Two implement units: `a` is a clean first pass; `b` fails once then
        // integrates (not a clean first pass). One per-unit review approves (`a`),
        // one gate runs twice (one fail then one pass = remediation noise), one
        // escalated unit `c`, plus an artifact-tagged verdict that must be ignored.
        let events = vec![
            started("a", "impl"),
            verdict("build", true),
            status("a", "verified"),
            status("a", "reviewed"),
            integrated("a"),
            artifact_verdict("build", "src/a.rs"), // GATED_BY bookkeeping - excluded
            started("b", "impl"),
            verdict("build", false),
            failed("b"),
            verdict("build", true),
            integrated("b"),
            started("c", "impl"),
            failed("c"),
            escalated("c"),
        ];
        let m = project(&events);

        assert_eq!(m.units_started, 3);
        assert_eq!(m.first_pass_clean, 1); // only `a`
        assert_eq!(m.first_pass_yield(), 1.0 / 3.0);

        let build = m.gates.get("build").expect("build gate tallied");
        // 3 real runs (a:pass, b:fail, b:pass) + the artifact verdict EXCLUDED.
        assert_eq!(build.pass, 2);
        assert_eq!(build.fail, 1);
        assert_eq!(build.total(), 3);

        assert_eq!(m.units_escalated, 1);
        assert_eq!(m.escalation_rate(), 1.0 / 3.0);

        assert_eq!(m.review_approve, 1); // `a` reviewed
        assert_eq!(m.review_reject, 0);
    }

    #[test]
    fn folds_a_synthetic_slice_into_a_fully_asserted_metrics_value() {
        // Spec/01 line 34: the projection is covered by a unit test that folds a
        // synthetic event slice and asserts EACH metric value. Where
        // `projects_every_metric_from_a_synthetic_run` checks fields piecemeal, this
        // test exercises ALL metrics at once - both review outcomes (approve AND
        // reject), two gates each with pass+fail, a clean first pass and a failed
        // one, and an escalation - then asserts the WHOLE `Metrics` value via a
        // single `Eq` against a fully-constructed expected struct. A field added to
        // `Metrics` later cannot silently escape this assertion the way it could a
        // set of per-field asserts.
        let events = vec![
            // `clean`: per-unit review approve, zero failures => first-pass clean.
            started("clean", "impl"),
            verdict("build", true),
            verdict("clippy", true),
            status("clean", "verified"),
            status("clean", "reviewed"),
            integrated("clean"),
            artifact_verdict("build", "src/clean.rs"), // GATED_BY bookkeeping - excluded
            // `reject`: per-unit review reject (verified then UnitFailed), retries,
            // then integrates - failed once so NOT a clean first pass.
            started("reject", "impl"),
            verdict("build", true),
            verdict("clippy", false),
            status("reject", "verified"),
            failed("reject"), // review reject (armed by `verified`)
            verdict("clippy", true),
            status("reject", "verified"),
            status("reject", "reviewed"),
            integrated("reject"),
            // `esc`: a named unit that fails and escalates - no review activity, so
            // its UnitFailed is a remediation, not a review reject.
            started("esc", "impl"),
            verdict("build", false),
            failed("esc"),
            escalated("esc"),
        ];

        let mut gates = BTreeMap::new();
        // build: clean(pass) + reject(pass) + esc(fail) = 2 pass / 1 fail. The
        // artifact-tagged build verdict is excluded.
        gates.insert("build".to_string(), GateCounts { pass: 2, fail: 1 });
        // clippy: clean(pass) + reject(fail) + reject(pass) = 2 pass / 1 fail.
        gates.insert("clippy".to_string(), GateCounts { pass: 2, fail: 1 });
        let expected = Metrics {
            units_started: 3,
            first_pass_clean: 1, // only `clean`
            gates,
            units_escalated: 1, // `esc`
            review_approve: 2,  // `clean` + `reject` (the retry approved)
            review_reject: 1,   // `reject`'s verified-then-UnitFailed
            // This slice carries no findings, adjudicator results, or sha stamps, so every
            // review-quality fold is empty - the new fields never disturb the existing ones.
            review_quality: ReviewQuality::default(),
        };

        let m = project(&events);
        // The whole value in one assertion - every metric, exactly.
        assert_eq!(m, expected);
        // Derived ratios over the same fold (never NaN; guarded denominators).
        assert_eq!(m.first_pass_yield(), 1.0 / 3.0);
        assert_eq!(m.escalation_rate(), 1.0 / 3.0);
        assert_eq!(m.gates["build"].total(), 3);
        assert_eq!(m.gates["clippy"].total(), 3);
    }

    #[test]
    fn counts_review_rejects_on_both_per_unit_and_fan_out_paths() {
        // Per-unit reject: `verified` arms, then UnitFailed-while-armed = reject.
        // Fan-out reject: empty-agent UnitStarted, then a bare UnitFailed = reject.
        let events = vec![
            started("p", "impl"),
            status("p", "verified"),
            failed("p"), // per-unit review reject
            started("f", ""),
            failed("f"), // fan-out / review-only reject
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 2);
        assert_eq!(m.review_approve, 0);
    }

    #[test]
    fn fan_out_reject_then_approve_counts_one_each() {
        // A fan-out review stage that rejects once (UnitFailed) and on the next
        // attempt approves (reviewed) yields exactly one reject and one approve.
        let events = vec![
            started("r", ""),
            failed("r"),             // reject
            status("r", "reviewed"), // approve on the retry
            integrated("r"),
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 1);
        assert_eq!(m.review_approve, 1);
    }

    #[test]
    fn a_gate_failure_of_a_named_unit_is_not_a_review_reject() {
        // A normal agent-named unit failing its gates (no `verified`, no `reviewed`)
        // is a remediation, NOT a review reject.
        let events = vec![
            started("g", "impl"),
            verdict("build", false),
            failed("g"),
            verdict("build", true),
            integrated("g"),
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 0);
        assert_eq!(m.review_approve, 0);
        assert_eq!(m.first_pass_clean, 0); // it failed once
    }

    #[test]
    fn a_spawn_crash_of_a_named_unit_is_not_a_review_reject() {
        // A unit that crashes before any gate/review (UnitFailed with no prior
        // verdict/verified) on a named agent is not a review reject.
        let events = vec![started("x", "impl"), failed("x")];
        let m = project(&events);
        assert_eq!(m.review_reject, 0);
    }

    #[test]
    fn agentless_gate_failure_is_counted_as_a_review_reject_known_false_positive() {
        // Pins the accepted LOSSY false positive (module doc): a non-fan-out stage
        // authored with an empty `agent` that runs real gates and fails emits a bare
        // UnitFailed (no verified, no reviewed) indistinguishable from a fan-out
        // reject, so it IS counted as a review reject. Exact classification would
        // need a `cause` field on UnitFailed, out of this read-model's scope.
        let events = vec![
            started("checkpoint", ""), // empty agent => review_only heuristic
            verdict("build", false),
            failed("checkpoint"),
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 1);
        assert_eq!(m.review_approve, 0);
        // The failing gate is still tallied as a real gate run.
        assert_eq!(m.gates.get("build").unwrap().fail, 1);
    }

    #[test]
    fn duplicate_unit_started_counts_the_unit_once() {
        // On resume the conductor re-emits UnitStarted for a not-yet-integrated
        // unit; the dedup guard keeps units_started (the yield/escalation
        // denominator) at 1.
        let events = vec![
            started("u", "impl"),
            failed("u"),
            started("u", "impl"), // resume re-emit
            integrated("u"),
        ];
        let m = project(&events);
        assert_eq!(m.units_started, 1);
        assert_eq!(m.first_pass_clean, 0); // failed before integrating
    }

    #[test]
    fn interleaved_units_keep_per_id_review_state() {
        // Two units' events interleave in the append-only stream (run_batch spawns
        // under a thread scope). Per-id state must not bleed: `a` approves, `b`
        // rejects.
        let events = vec![
            started("a", "impl"),
            started("b", "impl"),
            status("a", "verified"),
            status("b", "verified"),
            status("a", "reviewed"),
            integrated("a"),
            failed("b"), // b's verified arm => review reject for b
        ];
        let m = project(&events);
        assert_eq!(m.review_approve, 1);
        assert_eq!(m.review_reject, 1);
        assert_eq!(m.first_pass_clean, 1); // only `a`
    }

    #[test]
    fn escalation_is_counted_once_per_unit() {
        let events = vec![
            started("a", "impl"),
            started("b", "impl"),
            failed("a"),
            escalated("a"),
        ];
        let m = project(&events);
        assert_eq!(m.units_escalated, 1);
        assert_eq!(m.units_started, 2);
        assert_eq!(m.escalation_rate(), 0.5);
    }

    #[test]
    fn artifact_tagged_verdicts_are_excluded_from_gate_counts() {
        // Only the real (artifact-free) gate runs count; the per-file GATED_BY
        // verdicts emitted at integrate time are bookkeeping, not gate noise.
        let events = vec![
            verdict("clippy", true),
            artifact_verdict("clippy", "src/a.rs"),
            artifact_verdict("clippy", "src/b.rs"),
            verdict("clippy", false),
        ];
        let m = project(&events);
        let clippy = m.gates.get("clippy").unwrap();
        assert_eq!(clippy.pass, 1);
        assert_eq!(clippy.fail, 1);
        assert_eq!(clippy.total(), 2);
    }

    #[test]
    fn foreign_and_malformed_events_are_ignored_without_panicking() {
        // The shared log carries foreign types (DecisionMade) and, defensively,
        // malformed payloads. The fold must ignore them all and leave the metrics
        // for the valid run untouched - exercising the .ok()? / get()? sentinels.
        let events = vec![
            ev("DecisionMade", r#"{"id":"d","summary":"x"}"#), // foreign type
            ev(TYPE_UNIT_STARTED, "not json at all"),          // malformed payload
            ev(TYPE_UNIT_STARTED, r#"{"agent":"impl"}"#),      // id-less UnitStarted
            ev(TYPE_GATE_VERDICT, r#"{"pass":true}"#),         // gate-less verdict
            ev(TYPE_UNIT_STATUS, r#"{"id":"a"}"#),             // status-less
            started("a", "impl"),                              // the only valid unit
            verdict("build", true),
            status("a", "verified"),
            status("a", "reviewed"),
            integrated("a"),
        ];
        let m = project(&events);
        assert_eq!(m.units_started, 1);
        assert_eq!(m.first_pass_clean, 1);
        assert_eq!(m.review_approve, 1);
        assert_eq!(m.review_reject, 0);
        assert_eq!(m.gates.get("build").unwrap().pass, 1);
        // The gate-less malformed verdict must not have created an entry.
        assert_eq!(m.gates.len(), 1);
    }

    // ---- spec-11 unit-1 review-quality folds (each pinned by a synthetic log) ----

    /// A `verified`/`reviewed`/reject-`UnitFailed` event carrying the worktree-sha stamp.
    fn status_sha(id: &str, status_val: &str, sha: &str) -> Event {
        status(id, status_val).with_meta(META_WORKTREE_SHA, sha)
    }
    fn failed_sha(id: &str, sha: &str) -> Event {
        failed(id).with_meta(META_WORKTREE_SHA, sha)
    }
    /// A `ReviewFinding` a lens/adversary raised, attributed via the conductor-stamped
    /// META_ACTOR and concerning `about` files.
    fn finding(id: &str, actor: &str, about: &[&str]) -> Event {
        let about_json = about
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(",");
        ev(
            TYPE_REVIEW_FINDING,
            &format!(r#"{{"id":"{id}","about":[{about_json}]}}"#),
        )
        .with_meta(META_ACTOR, actor)
    }
    /// An adjudicator `SpawnResult` whose `output` carries the grown verdict JSON line.
    fn adjudication(unit: &str, attempt: u32, output: &str) -> Event {
        SpawnResult::ok(format!("{unit}/adjudicator#{attempt}"), output)
            .to_event()
            .unwrap()
    }
    /// A bare review `SpawnResult` of a given role, for per-tier spawn counting.
    fn review_spawn(unit: &str, role: &str, attempt: u32) -> Event {
        SpawnResult::ok(format!("{unit}/{role}#{attempt}"), "")
            .to_event()
            .unwrap()
    }

    #[test]
    fn flip_flop_counts_a_reject_then_approve_on_the_same_sha() {
        let events = vec![
            // `noise`: rejected on sha aaa, then APPROVED on the SAME sha aaa (the code
            // never changed) - a reviewer flip-flop.
            started("noise", "impl"),
            status_sha("noise", "verified", "aaa"),
            failed_sha("noise", "aaa"),
            status_sha("noise", "verified", "aaa"),
            status_sha("noise", "reviewed", "aaa"),
            integrated("noise"),
            // `real`: rejected on bbb, re-implemented, approved on a NEW sha ccc - a
            // legitimate remediation, NOT a flip-flop.
            started("real", "impl"),
            status_sha("real", "verified", "bbb"),
            failed_sha("real", "bbb"),
            status_sha("real", "verified", "ccc"),
            status_sha("real", "reviewed", "ccc"),
            integrated("real"),
        ];
        let m = project(&events);
        assert_eq!(m.review_reject, 2);
        assert_eq!(m.review_quality.flip_flops, 1);
        assert!((m.flip_flop_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn finding_survival_is_upheld_over_raised_per_actor() {
        let events = vec![
            finding("f1", "architecture-reviewer", &["src/a.rs"]),
            finding("f2", "architecture-reviewer", &["src/b.rs"]),
            finding("f3", "sdet", &["src/c.rs"]),
            // The adjudicator upholds f1 and f3, discards f2.
            adjudication(
                "u",
                0,
                r#"{"verdict":"reject","upheld":["f1","f3"],"discarded":["f2"],"cause":"genuine-defect"}"#,
            ),
        ];
        let m = project(&events);
        let arch = m.review_quality.finding_survival["architecture-reviewer"];
        assert_eq!((arch.raised, arch.upheld), (2, 1));
        assert!((arch.survival() - 0.5).abs() < 1e-9);
        let sdet = m.review_quality.finding_survival["sdet"];
        assert_eq!((sdet.raised, sdet.upheld), (1, 1));
        assert_eq!(sdet.survival(), 1.0);
    }

    #[test]
    fn lens_overlap_counts_files_flagged_by_two_or_more_actors() {
        let events = vec![
            finding("f1", "architecture-reviewer", &["src/shared.rs"]),
            finding("f2", "sdet", &["src/shared.rs"]), // same file, a second actor => overlap
            finding("f3", "sdet", &["src/solo.rs"]),   // one actor only
        ];
        let m = project(&events);
        assert_eq!(m.review_quality.finding_files, 2); // shared.rs + solo.rs
        assert_eq!(m.review_quality.overlap_files, 1); // shared.rs
        assert!((m.review_quality.lens_overlap_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn rejections_and_escalations_split_by_adjudicator_cause() {
        let events = vec![
            started("u", "impl"),
            status("u", "verified"),
            adjudication(
                "u",
                0,
                r#"{"verdict":"reject","upheld":[],"discarded":[],"cause":"spec-ambiguity"}"#,
            ),
            failed("u"), // reject #1: spec-ambiguity
            status("u", "verified"),
            adjudication(
                "u",
                1,
                r#"{"verdict":"reject","upheld":[],"discarded":[],"cause":"genuine-defect"}"#,
            ),
            failed("u"), // reject #2 (final): genuine-defect
            escalated("u"),
        ];
        let m = project(&events);
        assert_eq!(m.review_quality.rejections_by_cause["spec-ambiguity"], 1);
        assert_eq!(m.review_quality.rejections_by_cause["genuine-defect"], 1);
        // The escalation inherits the cause of the unit's FINAL rejection only.
        assert_eq!(m.review_quality.escalations_by_cause["genuine-defect"], 1);
        assert_eq!(
            m.review_quality.escalations_by_cause.get("spec-ambiguity"),
            None
        );
    }

    #[test]
    fn adversary_precision_is_over_adversary_only_findings() {
        let events = vec![
            // Adversary-only, on a file no lens flagged - UPHELD.
            finding("a1", "adversary", &["src/only.rs"]),
            // Adversary finding on a file a lens ALSO flagged - NOT adversary-only.
            finding("a2", "adversary", &["src/shared.rs"]),
            finding("l1", "sdet", &["src/shared.rs"]),
            // Adversary-only, on its own file - DISCARDED (not upheld).
            finding("a3", "adversary", &["src/other.rs"]),
            adjudication(
                "u",
                0,
                r#"{"verdict":"approve","upheld":["a1","a2"],"discarded":["a3"]}"#,
            ),
        ];
        let m = project(&events);
        // adversary-only set = {a1 upheld, a3 not}; a2 is excluded (shared with a lens).
        let adv = m.review_quality.adversary_only;
        assert_eq!((adv.raised, adv.upheld), (2, 1));
        assert!((m.review_quality.adversary_precision() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn cost_per_upheld_finding_is_spawns_over_upheld_per_tier() {
        let events = vec![
            review_spawn("u", "lens:architecture-reviewer", 0),
            review_spawn("u", "lens:sdet", 0),
            review_spawn("u", "adversary", 0),
            finding("f1", "architecture-reviewer", &["src/a.rs"]),
            finding("f2", "sdet", &["src/b.rs"]),
            finding("f3", "adversary", &["src/c.rs"]),
            // Upholds one lens finding (f1) and the adversary's (f3); discards f2.
            adjudication(
                "u",
                0,
                r#"{"verdict":"reject","upheld":["f1","f3"],"discarded":["f2"],"cause":"genuine-defect"}"#,
            ),
        ];
        let m = project(&events);
        let lens = m.review_quality.tier_cost["lens"];
        assert_eq!((lens.spawns, lens.upheld), (2, 1)); // 2 lens spawns, only f1 upheld
        assert!((lens.cost_per_upheld() - 2.0).abs() < 1e-9);
        let adv = m.review_quality.tier_cost["adversary"];
        assert_eq!((adv.spawns, adv.upheld), (1, 1));
        assert_eq!(adv.cost_per_upheld(), 1.0);
        // The adjudicator SpawnResult is counted as a tier spawn too - it judges, it raises
        // no findings, so its upheld is 0 (a cost with no findings of its own to survive).
        let adj = m.review_quality.tier_cost["adjudicator"];
        assert_eq!((adj.spawns, adj.upheld), (1, 0));
    }

    #[test]
    fn old_contract_log_without_shas_or_grown_verdicts_folds_empty_and_never_nan() {
        // Backward compatibility: a pre-spec-11 run stamps no worktree sha and its
        // adjudicator emits only `{"verdict":...}` (no upheld/discarded/cause). Every
        // review-quality fold degrades to empty - never a panic, never a NaN rate.
        let events = vec![
            started("u", "impl"),
            status("u", "verified"),
            failed("u"), // reject, but no sha to pair - no flip-flop bookkeeping
            status("u", "verified"),
            status("u", "reviewed"), // approve, no sha
            integrated("u"),
            adjudication("u", 2, r#"{"verdict":"approve"}"#),
        ];
        let m = project(&events);
        assert_eq!(m.review_quality.flip_flops, 0);
        assert!(m.review_quality.finding_survival.is_empty());
        assert!(m.review_quality.rejections_by_cause.is_empty());
        assert_eq!(m.review_quality.finding_files, 0);
        assert_eq!(m.flip_flop_rate(), 0.0);
        assert_eq!(m.review_quality.lens_overlap_rate(), 0.0);
        assert_eq!(m.review_quality.adversary_precision(), 0.0);
        // Only the adjudicator spawn is tallied as a tier cost (no findings to survive).
        assert_eq!(
            m.review_quality.tier_cost.get("adjudicator"),
            Some(&TierCost {
                spawns: 1,
                upheld: 0
            })
        );
    }

    #[test]
    fn finding_by_field_attributes_actor_when_no_meta_actor_stamp() {
        // The actor resolves from the payload `by` when META_ACTOR is absent, mirroring
        // the context-graph fold's provenance rule.
        let f = ev(
            TYPE_REVIEW_FINDING,
            r#"{"id":"f1","by":"sdet","about":["src/a.rs"]}"#,
        );
        let m = project(&[
            f,
            adjudication(
                "u",
                0,
                r#"{"verdict":"reject","upheld":["f1"],"cause":"genuine-defect"}"#,
            ),
        ]);
        assert_eq!(m.review_quality.finding_survival["sdet"].upheld, 1);
    }
}
