//! Spawn requests: the stepwise/replay driver's unit of work.
//!
//! Each request carries a DETERMINISTIC id derived from its position in the run
//! structure (unit id + stage/role + attempt) - never wall clock, randomness, or an
//! in-memory counter - so a step process that re-runs the conductor over recorded
//! history computes the SAME id for the SAME spawn and matches a recorded result
//! back to the call that produced it, across process boundaries (§4, spec 04).
//!
//! A request also carries everything the thin native driver needs to actually run
//! the agent: the grounded task `prompt`, the agent's `system_prompt` (its persona /
//! role instructions), its `model` alias, its granted `tools`, its working `dir`,
//! and the `unit` id + `stage` for the per-unit progress label the driver builds.
//!
//! When a step reaches an UNRECORDED spawn at the frontier it PARKS the call: the
//! request is persisted to the run's event log as a [`TYPE_SPAWN_REQUESTED`] event
//! (this module owns that serialization). The replay driver reads them back with
//! [`recorded`] to answer already-recorded spawns from the log, and the spawn-budget
//! breaker counts those same DISTINCT requests (via [`recorded`], deduped by id - a
//! re-parked id is never double-counted) - so both derive from the log rather than
//! process memory, and the breaker binds across every step process the run spans.
//!
//! This is DISTINCT from `driver::workflow::SpawnRequest`, the in-process MCP
//! driver's wire type: that path (the shim) keeps working unchanged, while this
//! vocabulary is what the stepwise `rigger step` / `rigger result` surface persists.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::eventstore::Event;
use crate::ledger::AttentionEntry;

/// Filesystem prefix of a unit's DETERMINISTIC worktree dir under the scratch root
/// (`rigger-wt-<slug>`); the conductor's `unit_worktree_dir` is the single authority that
/// builds it, and [`crate::worktree`]'s sweep / [`unit_cache_sibling`] read it back.
///
/// Defined here (spec 93, criterion 1) rather than in [`crate::worktree`] because
/// [`WaveItem::from`] - a pure fold - needs [`unit_cache_sibling`], and `worktree` is a
/// `store`-gated module (real git/filesystem operations) excluded from the `core` lane;
/// [`crate::worktree`] re-exports both so its own 30-odd call sites are unaffected.
pub const UNIT_WORKTREE_PREFIX: &str = "rigger-wt-";

/// Filesystem prefix of a unit's per-unit build cache dir (`cargo-target-<slug>`), a
/// SIBLING of its worktree under the scratch root (Gap 19). See [`UNIT_WORKTREE_PREFIX`]
/// for why this lives here rather than in [`crate::worktree`].
pub const UNIT_CACHE_PREFIX: &str = "cargo-target-";

/// The per-unit build cache dir that is a SIBLING of the unit worktree at `worktree_dir`
/// (Gap 19): `<root>/rigger-wt-<slug>` -> `<root>/cargo-target-<slug>`. Returns None for any
/// dir that is not a unit worktree (e.g. a `rigger-review-*` review worktree, or the empty
/// worktree-less path), which owns no such cache. Because both the worktree dir and the cache
/// dir derive from the same scratch root and the same slug, swapping the prefix reconstructs
/// the exact cache path. This is the SINGLE source of the derivation: the conductor's
/// `run_gates` uses it to point a gate's `CARGO_TARGET_DIR` at the cache, and
/// `crate::worktree::reclaim_cache_sibling` uses it to reclaim that same cache when the
/// worktree is removed. Pure path arithmetic (see [`UNIT_WORKTREE_PREFIX`]'s doc for why it
/// is defined here, not in `worktree`).
pub fn unit_cache_sibling(worktree_dir: &str) -> Option<String> {
    let path = std::path::Path::new(worktree_dir);
    let slug = path
        .file_name()?
        .to_str()?
        .strip_prefix(UNIT_WORKTREE_PREFIX)?;
    let parent = path.parent()?.to_str()?;
    Some(format!("{parent}/{UNIT_CACHE_PREFIX}{slug}"))
}

/// Filesystem prefix of a unit's per-unit mutants-root dir (`cargo-mutants-<slug>`), a
/// SIBLING of its worktree under the scratch root (spec 91, THE GATE ENVIRONMENT) - the
/// exact same sibling shape as [`UNIT_CACHE_PREFIX`]'s `cargo-target-<slug>`. See
/// [`UNIT_WORKTREE_PREFIX`]'s doc for why this lives here rather than in `worktree`.
pub const UNIT_MUTANTS_PREFIX: &str = "cargo-mutants-";

/// The per-unit mutants-root dir that is a SIBLING of the unit worktree at `worktree_dir`
/// (spec 91): `<root>/rigger-wt-<slug>` -> `<root>/cargo-mutants-<slug>`, exported to the
/// `checkin` stage's `mutation` gate command as `$MUTANTS` (mirroring how
/// [`unit_cache_sibling`] is exported as `CARGO_TARGET_DIR`). Returns `None` for any dir
/// that is not a unit worktree - the identical shape and identical `None` cases as
/// [`unit_cache_sibling`], just a different sibling name.
pub fn unit_mutants_sibling(worktree_dir: &str) -> Option<String> {
    let path = std::path::Path::new(worktree_dir);
    let slug = path
        .file_name()?
        .to_str()?
        .strip_prefix(UNIT_WORKTREE_PREFIX)?;
    let parent = path.parent()?.to_str()?;
    Some(format!("{parent}/{UNIT_MUTANTS_PREFIX}{slug}"))
}

/// The event type a parked spawn request is persisted as - the "spawn-request" half
/// of the spawn-request/result pair the spec permits as the only new vocabulary the
/// stepwise driver needs. It is deliberately NOT one of the run-lifecycle events the
/// ledger folds, so an unknown-event-ignoring projection (the ledger, the context
/// graph) skips it and only [`recorded`] and the replay driver read it.
pub const TYPE_SPAWN_REQUESTED: &str = "SpawnRequested";

/// The role token for the unit's implementer (the stage's own `agent`).
pub const ROLE_IMPLEMENTER: &str = "implementer";
/// The role token for a unit review's tier-2 adversary.
pub const ROLE_ADVERSARY: &str = "adversary";
/// The role token for a unit review's tier-3 adjudicator (the gating verdict).
pub const ROLE_ADJUDICATOR: &str = "adjudicator";
/// The role token for the SDET periphery-test AUTHOR (spec 32): the write-capable role
/// that authors the periphery test layer (contract / API / integration) at the build
/// seam - after the implementer emits green and before the pre-gate commit - so no
/// boundary surface a unit exposes lands untested. A FIRST-CLASS role like the
/// implementer and the reviewers (its agent id `sdet-author` doubles as the token, as
/// with the adversary/adjudicator), and DISTINCT from the read-only `sdet` review LENS
/// (`lens:sdet`), which reviews the implementer's code and cannot write.
pub const ROLE_SDET_AUTHOR: &str = "sdet-author";

/// The role token for a tier-1 review lens. A stage runs several lenses in parallel,
/// so the lens's own agent id disambiguates them within one attempt: two lenses on
/// the same unit+attempt get distinct spawn ids because their role tokens differ.
///
/// ```
/// # use rigger::spawn::lens_role;
/// assert_eq!(lens_role("sdet"), "lens:sdet");
/// ```
pub fn lens_role(agent_id: &str) -> String {
    format!("lens:{agent_id}")
}

/// Derive a spawn's DETERMINISTIC id from its position in the run structure: the
/// `unit` id, the stage/`role` token, and the 0-based remediation `attempt`.
///
/// The id is a PURE function of these three coordinates - no wall clock, no
/// randomness, no in-memory counter - so two step processes replaying the same
/// recorded history compute the identical id for the identical spawn, which is what
/// lets a recorded result be matched back to the call that produced it across
/// processes (§4, spec 04).
///
/// A stage produces at most one spawn per role per attempt, so the triple
/// `(unit, role, attempt)` names a spawn uniquely. The id is kept human-readable
/// (rather than an opaque hash) because it is the handle a courier passes to
/// `rigger result <id>`. Unit ids and role tokens are drawn from the run structure
/// (kebab identifiers and the fixed role vocabulary above), neither of which
/// contains the `/` or `#` separators, so the rendering is unambiguous.
///
/// ```
/// # use rigger::spawn::{spawn_id, lens_role, ROLE_IMPLEMENTER};
/// assert_eq!(spawn_id("spawn-req", ROLE_IMPLEMENTER, 0), "spawn-req/implementer#0");
/// assert_eq!(spawn_id("spawn-req", &lens_role("sdet"), 2), "spawn-req/lens:sdet#2");
/// ```
pub fn spawn_id(unit: &str, role: &str, attempt: u32) -> String {
    format!("{unit}/{role}#{attempt}")
}

/// Derive the spawn id for a reviewer RESPAWN (Gap 18, spec 07). A reviewer spawn
/// (lens, adversary, or adjudicator) whose recorded result is empty or whitespace-only
/// is an INFRASTRUCTURE fault, not a verdict, so the conductor respawns the SAME
/// reviewer under a NEW deterministic id. `retry` is the 0-based respawn ordinal:
/// `retry == 0` is the reviewer's ORIGINAL spawn and returns the plain [`spawn_id`]
/// unchanged (so the normal, non-degenerate path keeps its exact id and nothing else
/// moves), while each `retry > 0` appends a deterministic `~retry{n}` suffix - `~` is
/// neither the `/` nor the `#` the id structure reserves, so the respawn id stays
/// unambiguous and is the same human-readable `rigger result <id>` handle a courier
/// uses. Each respawn thus gets a DISTINCT id a stepwise/replay driver parks and
/// answers independently of the original.
///
/// Like [`spawn_id`] it is a PURE function of its coordinates - unit + role + attempt +
/// retry ordinal - with no wall clock, no randomness, and no in-memory counter, so two
/// step processes replaying the same recorded history compute the identical retry id
/// (the spec 07 Gap-18 replay-safety constraint: every new spawn id derives
/// deterministically from unit + role + attempt + retry ordinal).
///
/// ```
/// # use rigger::spawn::{spawn_retry_id, spawn_id, ROLE_ADJUDICATOR};
/// assert_eq!(
///     spawn_retry_id("u", ROLE_ADJUDICATOR, 1, 0),
///     spawn_id("u", ROLE_ADJUDICATOR, 1),
/// );
/// assert_eq!(spawn_retry_id("u", ROLE_ADJUDICATOR, 1, 2), "u/adjudicator#1~retry2");
/// ```
pub fn spawn_retry_id(unit: &str, role: &str, attempt: u32, retry: u32) -> String {
    let base = spawn_id(unit, role, attempt);
    if retry == 0 {
        base
    } else {
        format!("{base}~retry{retry}")
    }
}

/// The deterministic id of a unit's first-green-wins speculation GROUP (spec 13, unit 3):
/// the single correlation handle that ties a unit's K parallel implementer candidates
/// together. The winner's `UnitIntegrated` and every cancelled candidate's status carry
/// it, so the group / winner / losers are recoverable from the log without a new event
/// type. The K candidates themselves keep ordinary per-attempt implementer ids
/// ([`spawn_id`]`(unit, `[`ROLE_IMPLEMENTER`]`, lane)`) - candidate `lane` runs at
/// attempt `lane`, so each candidate's gates, statuses, and review tiers key apart while
/// remaining a PURE function of the run structure (no wall clock, no randomness), the
/// same replay-determinism [`spawn_id`] guarantees.
///
/// ```
/// # use rigger::spawn::speculation_group_id;
/// assert_eq!(speculation_group_id("u"), "u/spec-group");
/// ```
pub fn speculation_group_id(unit: &str) -> String {
    format!("{unit}/spec-group")
}

/// The ROLE token of a spawn id `{unit}/{role}#{attempt}` (a `~retry{n}` respawn suffix
/// and the `#{attempt}` ordinal both trimmed), or the whole id when it carries no `/`.
/// The inverse of the `{unit}/{role}` half [`spawn_id`] renders, and the single authority
/// the review-tier and disposition folds share for recovering a spawn's role.
///
/// ```
/// # use rigger::spawn::{spawn_role, ROLE_ADJUDICATOR};
/// assert_eq!(spawn_role("u1/adjudicator#0"), ROLE_ADJUDICATOR);
/// assert_eq!(spawn_role("u1/adjudicator#0~retry2"), ROLE_ADJUDICATOR);
/// assert_eq!(spawn_role("u1/lens:sdet#1"), "lens:sdet");
/// ```
pub fn spawn_role(id: &str) -> &str {
    let role = id.rsplit_once('/').map(|(_, r)| r).unwrap_or(id);
    role.split(['#', '~']).next().unwrap_or(role)
}

/// The UNIT token of a spawn id `{unit}/{role}#{attempt}` - everything before the final `/`, the
/// exact inverse of the `{unit}/{role}` half [`spawn_id`] renders and the companion of
/// [`spawn_role`]. A unit id carries no `/` (and a `~retry{n}` respawn suffix rides the role, after
/// the `/`), so the last `/` cleanly separates unit from role; a bare id with no `/` names no unit
/// and yields `None`. Kept beside [`spawn_role`] so the id grammar has ONE owner: a reader that
/// needs the emitting unit of a spawn-stamped event never re-parses the `/` in a view adapter.
///
/// ```
/// # use rigger::spawn::unit_of;
/// assert_eq!(unit_of("u1/implementer#0"), Some("u1"));
/// assert_eq!(unit_of("u43-c1-machinery-gone/lens:sdet#1"), Some("u43-c1-machinery-gone"));
/// assert_eq!(unit_of("u1/adjudicator#1~retry2"), Some("u1"));
/// assert_eq!(unit_of("bare-id"), None);
/// ```
pub fn unit_of(id: &str) -> Option<&str> {
    id.rsplit_once('/').map(|(unit, _)| unit)
}

/// The remediation ATTEMPT ordinal of a spawn id `{unit}/{role}#{attempt}` (a `~retry{n}`
/// respawn suffix trimmed first), or `0` when the id carries no `#{attempt}`. The inverse of
/// the `#{attempt}` ordinal [`spawn_id`] mints - kept here beside [`spawn_role`] so the id
/// grammar has ONE owner: a reader never re-parses `#`/`~retry` in a view adapter, which would
/// silently diverge if the separators ever moved with the struct.
///
/// ```
/// # use rigger::spawn::attempt_of;
/// assert_eq!(attempt_of("u1/implementer#0"), 0);
/// assert_eq!(attempt_of("u1/implementer#2"), 2);
/// assert_eq!(attempt_of("u1/adjudicator#1~retry3"), 1);
/// assert_eq!(attempt_of("no-ordinal"), 0);
/// ```
pub fn attempt_of(id: &str) -> u32 {
    id.rsplit('#')
        .next()
        .and_then(|s| s.split('~').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// The Gap-18 RESPAWN ordinal of a reviewer id `{unit}/{role}#{attempt}~retry{n}` (spec 07):
/// the `n` after `~retry`, or `0` for an ORIGINAL spawn carrying no `~retry` suffix. Kept
/// DISTINCT from [`attempt_of`] (which a respawn SHARES with its original) so a caller can tell
/// a respawn from its original without conflating the two ordinals; the inverse of the `~retry`
/// suffix [`spawn_retry_id`] mints, owned here alongside the rest of the id grammar.
///
/// ```
/// # use rigger::spawn::retry_of;
/// assert_eq!(retry_of("u1/adjudicator#1"), 0);
/// assert_eq!(retry_of("u1/adjudicator#1~retry2"), 2);
/// ```
pub fn retry_of(id: &str) -> u32 {
    id.rsplit_once("~retry")
        .and_then(|(_, n)| n.parse().ok())
        .unwrap_or(0)
}

/// An adjudicator's DISPOSITION, parsed from its recorded verdict line (spec 11): the
/// finding ids it UPHELD and the rejection `cause`. The single source both the
/// review-quality metric ([`crate::metrics`] finding-survival) and the context-graph
/// finding-expiry ([`crate::contextgraph`], spec 25) read, so the two never diverge on
/// what a review resolved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adjudication {
    /// The finding ids the adjudicator upheld (its `upheld` array) - the review-quality
    /// metric's finding-survival numerator. NOT the inverse of [`discarded`](Self::discarded):
    /// a verdict may uphold nothing yet discard nothing (an approve carrying neither), and a
    /// finding named in neither is still-open, not discarded.
    pub upheld: Vec<String>,
    /// The finding ids the adjudicator DISCARDED (its `discarded` array): the review raised
    /// them but the verdict resolved them as not-to-carry. The EXPLICIT disposition the
    /// context-graph finding-expiry keys on - never the complement of [`upheld`](Self::upheld),
    /// so a verdict that omits `upheld` never sweeps a review's still-open findings and a
    /// reject's own motivating findings stay live unless it named them here.
    pub discarded: Vec<String>,
    /// The rejection `cause`, present only on a reject verdict (`None` on approve or when
    /// the adjudicator declared none).
    pub cause: Option<String>,
    /// The raw `verdict` literal itself (`"approve"` or `"reject"`), `None` when the parsed
    /// line carried no `verdict` field at all (an old-contract line naming only `upheld`/
    /// `discarded`). Kept alongside [`cause`](Self::cause) rather than folded away: a
    /// consumer that needs approve-vs-reject (spec 94 c3's `console::scrub_track` mark
    /// colour) reads this directly instead of inferring it from `cause`'s presence, which
    /// is silent on a reject that declared no cause.
    pub verdict: Option<String>,
}

/// A single spawn request: one agent to run, plus the deterministic id that names it
/// and the display labels the thin driver groups its progress under.
///
/// Serializes to the exact JSON that `rigger step` prints in a wave AND that is
/// persisted as the [`TYPE_SPAWN_REQUESTED`] event body - one shape, so a wave read
/// off the log and a wave printed to a driver are byte-identical. Empty optional
/// fields are omitted from the wire to keep the persisted event compact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// The deterministic id (see [`spawn_id`]): `{unit}/{role}#{attempt}`.
    pub id: String,
    /// The unit this spawn belongs to - the display label's unit half and the
    /// correlation key the replay driver and budget breaker group spawns under.
    pub unit: String,
    /// The stage that produced this spawn - the display label's stage half. The thin
    /// driver builds a per-unit `opts.phase` label from `unit` + `stage`.
    pub stage: String,
    /// The grounded task prompt the agent runs (its user-turn instruction).
    pub prompt: String,
    /// The agent's PERSONA - its role instructions (`AgentDef::prompt`), threaded
    /// from the conductor's single persona source. It is the agent's SYSTEM prompt,
    /// distinct from the task `prompt`. Omitted from the wire when empty (an agent
    /// that declared no body).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub system_prompt: String,
    /// The model alias the agent runs on (e.g. `"sonnet"`); empty inherits the
    /// driver's default model.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// The tools the agent is granted - already fan-out-stripped by
    /// `AgentDef::allowed_tools` when the agent is not `recurse`, so a spawned agent
    /// cannot spawn sub-agents (§3.1, §6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// The working dir the agent runs in: an isolated worktree, or empty for the
    /// current dir.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    /// The agent's blast-radius - the grounded seed files this spawn is scoped to
    /// (§5.3). The thin driver carries it to `rigger peers` to scope the
    /// tool-boundary injection of peer decisions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blast_radius: Vec<String>,
    /// The per-spawn wall-clock bound in SECONDS (spec 10, unit 3), resolved from the
    /// agent's `max_wall_clock` (its own, or the inherited `defaults.max_wall_clock`).
    /// `rigger step` treats this spawn as a hung/infra fault once its liveness marker is
    /// stale longer than this, and the driver frames the worker's heartbeat around it.
    /// `None` (the common back-compatible case) means unbounded - never timed out - and
    /// is omitted from the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_clock: Option<u64>,
    /// The spawn's LIVE WORK-LINE (spec 19a, c4): the unit's criterion (its
    /// [`Stage::coverage`](crate::config::Stage), trimmed), threaded from the conductor so
    /// the thin driver can narrate the actual WORK a worker is doing - not just its
    /// `{unit}:{stage}` progress-group label. Carried onto [`WaveItem`] so the printed wave
    /// `rigger step` emits surfaces it for `workflows/rigger.js` to render. Omitted from the
    /// wire when empty (a spawn with no criterion - e.g. a plan/canary stage - serializes
    /// exactly as before), so it is a purely additive, back-compatible field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The routed review roster a review-tier spawn (adversary/adjudicator) judges (spec 67,
    /// criterion 4): the unit's lens role tokens (e.g. `["lens:sdet",
    /// "lens:architecture-reviewer"]`) for the adversary, plus [`ROLE_ADVERSARY`] for the
    /// adjudicator - stamped by the CONDUCTOR from the actually-routed panel (light or full),
    /// never guessed by the driver, so a replayed or tier-reduced roster is never stale.
    /// Carried onto [`WaveItem`] so the printed wave `rigger step` emits surfaces it for
    /// `workflows/rigger.js` to render inside the action phrase. Omitted from the wire when
    /// empty (a lens spawn, an empty panel, or an older conductor) - a purely additive,
    /// back-compatible field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviews: Vec<String>,
}

impl SpawnRequest {
    /// Serialize this request as its [`TYPE_SPAWN_REQUESTED`] event, ready to append
    /// to the run stream.
    pub fn to_event(&self) -> Result<Event, serde_json::Error> {
        Ok(Event::new(TYPE_SPAWN_REQUESTED, serde_json::to_vec(self)?))
    }

    /// Recover a request from a [`TYPE_SPAWN_REQUESTED`] event body.
    pub fn from_event(e: &Event) -> Result<SpawnRequest, serde_json::Error> {
        serde_json::from_slice(&e.data)
    }
}

/// A minimal request for the crate's unit tests: the deterministic id derived from `unit` +
/// `role` + `attempt` (so it cannot drift from the labels), every optional field empty.
#[cfg(test)]
pub(crate) fn test_request(
    unit: &str,
    stage: &str,
    role: &str,
    attempt: u32,
    prompt: &str,
) -> SpawnRequest {
    SpawnRequest {
        id: spawn_id(unit, role, attempt),
        unit: unit.to_string(),
        stage: stage.to_string(),
        prompt: prompt.to_string(),
        ..SpawnRequest::default()
    }
}

/// The [`TYPE_SPAWN_REQUESTED`] events in `events`, still serialized - the ONE prefilter
/// [`recorded`], [`is_recorded`] and [`recorded_lenient`] all fold over, so a non-spawn
/// event is skipped in exactly one place rather than three times over.
fn spawn_requested_events(events: &[Event]) -> impl Iterator<Item = &Event> {
    events.iter().filter(|e| e.type_ == TYPE_SPAWN_REQUESTED)
}

/// Fold the [`TYPE_SPAWN_REQUESTED`] events in `events` into the spawn requests
/// already parked, keyed by their deterministic id.
///
/// The replay driver uses this to tell an already-recorded spawn (answer it from the
/// log) from an unrecorded one (park it); the budget breaker counts the entries.
/// Non-spawn events are ignored, so the same run stream feeds this and the
/// ledger/graph projections. A re-parked id (an idempotency violation the replay
/// driver is responsible for preventing) collapses to the last-written request. A
/// malformed spawn body is a genuine invariant violation for both callers, so this
/// propagates the parse error rather than skipping it - see [`recorded_lenient`] for
/// the degrade-tolerant sibling a read-only display caller needs instead.
pub fn recorded(events: &[Event]) -> Result<BTreeMap<String, SpawnRequest>, serde_json::Error> {
    let mut out = BTreeMap::new();
    for e in spawn_requested_events(events) {
        let req = SpawnRequest::from_event(e)?;
        out.insert(req.id.clone(), req);
    }
    Ok(out)
}

/// Whether a spawn with `id` has already been parked in `events` - a cheap
/// membership check over [`recorded`] for the replay driver's park-or-replay
/// decision. A malformed spawn event never matches (it cannot carry a valid id).
pub fn is_recorded(events: &[Event], id: &str) -> bool {
    spawn_requested_events(events).any(|e| SpawnRequest::from_event(e).is_ok_and(|r| r.id == id))
}

/// Degrade-tolerant sibling of [`recorded`] (spec 94 c4, adj-u94c4-r3-verdict-reject-
/// recorded-spawn-duplication): the SAME fold over [`TYPE_SPAWN_REQUESTED`] events,
/// sharing [`spawn_requested_events`]'s own prefilter, but a malformed body is SKIPPED
/// rather than failing the whole enumeration. For a read-only DISPLAY caller
/// (`console::palette_commands`'s agent list) where one bad/older-run entry must lose
/// only its own row, never the whole reply - CONSTRAINTS WALK: "An older run lacking a
/// field - the fold renders the blank, never fails." [`recorded`]'s own `?` stays
/// correct for its OTHER callers (the replay driver's park-or-replay decision, the
/// budget breaker's hard count), where a malformed spawn is a genuine invariant
/// violation this function must never silently paper over - this is a second entry
/// point for a genuinely different caller contract, not a relaxation of that one.
pub fn recorded_lenient(events: &[Event]) -> BTreeMap<String, SpawnRequest> {
    let mut out = BTreeMap::new();
    for e in spawn_requested_events(events) {
        if let Ok(req) = SpawnRequest::from_event(e) {
            out.insert(req.id.clone(), req);
        }
    }
    out
}

/// The event type a recorded spawn RESULT is persisted as - the "result" half of the
/// spawn-request/result pair whose request half is [`TYPE_SPAWN_REQUESTED`]. Like the
/// request it is deliberately NOT one of the run-lifecycle events the ledger folds, so
/// an unknown-event-ignoring projection (the ledger, the context graph) skips it and
/// only [`result_of`] and the replay driver read it. `rigger result <id>` writes one;
/// the replay driver answers an already-recorded spawn by returning the matching
/// result instead of re-running the agent.
pub const TYPE_SPAWN_RESULT: &str = "SpawnResult";

/// The `--meta` object key by which a worker reports the RESOLVED model id that actually
/// served its spawn (spec 05 line 52): `rigger result <id> --meta '{"resolved_model": ..}'`
/// stores it in [`SpawnResult::meta`]. The conductor copies it off the replayed result onto
/// the spawn's unit events (see `conductor::META_MODEL_RESOLVED`), so the recorded events
/// name the concrete model that ran, not only the requested alias on the spawn request.
pub const META_RESOLVED_MODEL: &str = "resolved_model";

/// The [`SpawnResult::meta`] key `rigger step` stamps on a spawn it recorded as a LIVENESS
/// fault (spec 10, unit 3): its value is the failure CLASS the taxonomy assigned the hung
/// agent (e.g. `"infra"`). Its presence marks the result as a step-synthesized liveness
/// outcome - distinct from a worker-reported failure - so the replay driver re-parks it
/// (never charging the unit) and a later real result supersedes it (last-write-wins). No
/// new EVENT TYPE is introduced; the fault rides the existing [`SpawnResult`] on the
/// spawn's id, as the spec requires.
pub const META_LIVENESS_CLASS: &str = "liveness_class";

/// A recorded spawn OUTCOME, keyed by the same deterministic [`spawn_id`] as its
/// request. A successful run carries the agent's `output` and an empty `error`; a
/// failed run (`rigger result --error`) carries the failure message in `error`, and
/// the replay driver answers the spawn AS an error - so a step re-running the conductor
/// over recorded history sees the identical failure it saw live. `meta` carries the
/// optional `rigger result --meta <json>` courier bookkeeping.
///
/// Serializes with empty/null fields omitted, so a plain success result persists as
/// just `{"id":..,"output":..}`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpawnResult {
    /// The deterministic id of the spawn this answers (see [`spawn_id`]).
    pub id: String,
    /// The agent's output (its final message). Empty on an error result.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output: String,
    /// The failure message when the spawn errored; empty on success. A non-empty
    /// `error` makes the replay driver answer the spawn with a driver error, so a
    /// recorded failure stays a failure across step processes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// Optional courier metadata (`rigger result --meta <json>`); null when unset and
    /// then omitted from the wire.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub meta: Value,
}

impl SpawnResult {
    /// A SUCCESSFUL result: the agent finished and produced `output`.
    pub fn ok(id: impl Into<String>, output: impl Into<String>) -> SpawnResult {
        SpawnResult {
            id: id.into(),
            output: output.into(),
            error: String::new(),
            meta: Value::Null,
        }
    }

    /// A FAILED result (`rigger result --error`): the spawn errored with `error`. The
    /// replay driver answers a recorded failure with a driver error, never a fake
    /// success.
    pub fn failed(id: impl Into<String>, error: impl Into<String>) -> SpawnResult {
        SpawnResult {
            id: id.into(),
            output: String::new(),
            error: error.into(),
            meta: Value::Null,
        }
    }

    /// A LIVENESS-FAULT result `rigger step` records for a hung spawn (spec 10, unit 3):
    /// the `error` describes the stall and the [`META_LIVENESS_CLASS`] meta key carries the
    /// class the taxonomy assigned it (`class`). It is a step-synthesized outcome, not a
    /// worker report, so the replay driver re-parks it (charging no attempt) and a real
    /// result later supersedes it. Reuses the existing [`SpawnResult`] on the spawn's id -
    /// no new event type.
    pub fn liveness_fault(
        id: impl Into<String>,
        error: impl Into<String>,
        class: &str,
    ) -> SpawnResult {
        SpawnResult {
            id: id.into(),
            output: String::new(),
            error: error.into(),
            meta: serde_json::json!({ META_LIVENESS_CLASS: class }),
        }
    }

    /// Builder: attach the optional courier metadata (`rigger result --meta <json>`).
    pub fn with_meta(mut self, meta: Value) -> Self {
        self.meta = meta;
        self
    }

    /// Whether this result records a FAILURE (a non-empty `error`).
    pub fn is_error(&self) -> bool {
        !self.error.is_empty()
    }

    /// Whether this result is a step-synthesized LIVENESS fault (spec 10, unit 3): it
    /// carries the [`META_LIVENESS_CLASS`] meta key. The replay driver re-parks such a
    /// result instead of answering the spawn as a charged failure, so a hung agent never
    /// charges the unit a remediation attempt.
    pub fn is_liveness_fault(&self) -> bool {
        self.meta.get(META_LIVENESS_CLASS).is_some()
    }

    /// The string value of the `key` entry of [`meta`](SpawnResult::meta), or empty when the
    /// worker reported none (or reported a non-string value). The two keys it serves:
    ///
    /// - [`META_LIVENESS_CLASS`]: the liveness class recorded on a fault (empty when it is not
    ///   a liveness fault).
    /// - [`META_RESOLVED_MODEL`]: the RESOLVED model id the worker reported through `--meta`.
    ///   This is the concrete model that actually ran the spawn - distinct from the requested
    ///   alias on the spawn REQUEST - which the conductor copies onto the spawn's unit events
    ///   (spec 05 line 52).
    pub fn meta_str(&self, key: &str) -> String {
        self.meta
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    /// Whether this is an ADJUDICATOR result: the role token of its spawn id (see
    /// [`spawn_role`]) is [`ROLE_ADJUDICATOR`]. Only an adjudicator result carries a
    /// gating verdict, so only it disposes a review's findings.
    pub fn is_adjudicator(&self) -> bool {
        spawn_role(&self.id) == ROLE_ADJUDICATOR
    }

    /// Parse this ADJUDICATOR result's grown verdict line (spec 11) into its
    /// [`Adjudication`]: the LAST JSON object line of the output carrying a `verdict`,
    /// `upheld`, or `discarded` field yields the upheld and discarded finding ids and the
    /// rejection cause. Returns `None` when this is not an adjudicator result, or the output
    /// carries no verdict line (an old-contract adjudicator, or unparseable output) - the
    /// caller then disposes / attributes nothing. The single disposition-parse authority
    /// both the review-quality metric and the context-graph finding-expiry read.
    pub fn adjudication(&self) -> Option<Adjudication> {
        if !self.is_adjudicator() {
            return None;
        }
        for line in self.output.lines().rev() {
            let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            if v.get("verdict").is_none()
                && v.get("upheld").is_none()
                && v.get("discarded").is_none()
            {
                continue;
            }
            // One string-array reader for both id lists, so `upheld` and `discarded` can
            // never drift on how a verdict array is decoded.
            let str_array = |key: &str| -> Vec<String> {
                v.get(key)
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let cause = v
                .get("cause")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|s| !s.is_empty());
            let verdict = v.get("verdict").and_then(Value::as_str).map(str::to_owned);
            return Some(Adjudication {
                upheld: str_array("upheld"),
                discarded: str_array("discarded"),
                cause,
                verdict,
            });
        }
        None
    }

    /// Serialize this result as its [`TYPE_SPAWN_RESULT`] event, ready to append.
    pub fn to_event(&self) -> Result<Event, serde_json::Error> {
        Ok(Event::new(TYPE_SPAWN_RESULT, serde_json::to_vec(self)?))
    }

    /// Recover a result from a [`TYPE_SPAWN_RESULT`] event body.
    pub fn from_event(e: &Event) -> Result<SpawnResult, serde_json::Error> {
        serde_json::from_slice(&e.data)
    }
}

/// The LATEST recorded result for `id`, or `None` if the spawn has no result yet (it is
/// still parked at the frontier, awaiting a courier's `rigger result`). This is how the
/// replay driver decides answer-vs-park: `Some` answers the spawn, `None` parks it.
///
/// Later results win, so a corrected re-record supersedes an earlier one. Non-result
/// events (and malformed result bodies via the surfaced error) are handled just like
/// [`recorded`], so the same run stream feeds this and the ledger/graph projections.
pub fn result_of(events: &[Event], id: &str) -> Result<Option<SpawnResult>, serde_json::Error> {
    let mut found = None;
    for e in events {
        if e.type_ == TYPE_SPAWN_RESULT {
            let res = SpawnResult::from_event(e)?;
            if res.id == id {
                found = Some(res);
            }
        }
    }
    Ok(found)
}

/// One wave entry as `rigger step` prints it: the SLIM manifest of a parked spawn -
/// everything the thin driver needs to LAUNCH the agent (identity, placement, model),
/// and nothing it doesn't. The prompt and persona are deliberately ABSENT: they can be
/// hundreds of kilobytes each (a review-round's accumulated context), and the wave
/// transits a model-relayed structured output where megabyte payloads cannot survive
/// verbatim. The worker fetches its own prompt from the log by spawn id
/// (`rigger prompt <id>`) - the store is the channel, the wave is a reference.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct WaveItem {
    pub id: String,
    pub unit: String,
    pub stage: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    // `dir`, `max_wall_clock`, `marker_path` and `cargo_target_dir` are ALWAYS on the wire
    // (empty or null when absent), never skipped: the wave reaches the driver through a
    // courier agent's structured return, and a key the driver's schema cannot REQUIRE is a
    // key the courier can drop while retyping - it dropped `marker_path` and
    // `cargo_target_dir` on 2026-09-15, costing a worker its heartbeat and its build location.
    #[serde(default)]
    pub dir: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blast_radius: Vec<String>,
    /// The per-spawn wall-clock bound in SECONDS (spec 10, unit 3), carried to the thin
    /// driver so it can frame the worker's heartbeat and watchdog a hung agent. Omitted
    /// from the wire when the spawn is unbounded.
    #[serde(default)]
    pub max_wall_clock: Option<u64>,
    /// The RESOLVED absolute path of this spawn's liveness marker (spec 10, unit 3), stamped
    /// by `rigger step` from the SINGLE authority [`crate::liveness::marker_path`] over the
    /// step's own resolved scratch root (`RIGGER_TMPDIR` > `defaults.workdir` > repo default)
    /// and run id. Carrying it on the wire is what keeps the worker-write path IDENTICAL to
    /// the sweep-read path under ANY scratch config: the thin driver frames both the
    /// heartbeat `touch` and its staleness watchdog around THIS path and never re-derives a
    /// root of its own. Present only for a bounded spawn (a marker exists only when
    /// `max_wall_clock` is set); [`WaveItem::from`] leaves it `None` because the scratch root
    /// and run id are not known to a pure fold - `rigger step` fills it in.
    #[serde(default)]
    pub marker_path: Option<String>,
    /// The unit's ONE build location (spec 77, criterion 1): the `cargo-target-<unit>`
    /// sibling of the worktree, the same directory the unit's gates build into. The SDK
    /// driver receives it as `CARGO_TARGET_DIR` in the spawn's environment; a driver that
    /// cannot set a worker's environment (the editor's workflow driver runs workers through
    /// an agent tool with none) must NAME it in the worker's instructions instead, or every
    /// `cargo test` the worker runs lands in `<worktree>/target` - three such trees filled
    /// the disk on 2026-09-15. Absent when the spawn has no unit worktree (a review or
    /// plan spawn inherits the shared cache).
    #[serde(default)]
    pub cargo_target_dir: Option<String>,
    /// The spawn's LIVE WORK-LINE (spec 19a, c4): the unit's criterion, copied from the
    /// [`SpawnRequest::title`] so it rides the SLIM manifest the thin driver actually reads.
    /// The wave `rigger step` prints is a `Vec<WaveItem>`, NOT the request, so a title that
    /// lived only on the request would render NOTHING - `workflows/rigger.js` narrates from
    /// this field. Omitted from the wire when empty (an untitled spawn stays byte-identical
    /// to the historical slim manifest).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The routed review roster, copied from [`SpawnRequest::reviews`] so it rides the SLIM
    /// manifest the thin driver actually reads (the same title-copy seam this field mirrors -
    /// spec 67, criterion 4). Omitted from the wire when empty (a lens spawn, an empty panel,
    /// or an older conductor stays byte-identical to the historical slim manifest).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviews: Vec<String>,
}

impl From<&SpawnRequest> for WaveItem {
    fn from(req: &SpawnRequest) -> Self {
        WaveItem {
            id: req.id.clone(),
            unit: req.unit.clone(),
            stage: req.stage.clone(),
            model: req.model.clone(),
            tools: req.tools.clone(),
            dir: req.dir.clone(),
            blast_radius: req.blast_radius.clone(),
            max_wall_clock: req.max_wall_clock,
            // Stamped by `rigger step` (cmd_step) from the resolved scratch root + run id;
            // a pure fold has neither, so leave it absent here.
            marker_path: None,
            // The same derivation `spawn_env` uses for the SDK driver's CARGO_TARGET_DIR, so
            // both drivers name one build location per unit.
            cargo_target_dir: unit_cache_sibling(&req.dir),
            // The live work-line rides the slim manifest: the wave the thin driver reads is
            // a `Vec<WaveItem>`, so the title MUST be copied here or `rigger.js` narrates
            // nothing (the false-green class this copy closes).
            title: req.title.clone(),
            // Same seam, same reason: the routed review roster must ride the slim manifest or
            // the driver has nothing to render inside the action phrase (spec 67, criterion 4).
            reviews: req.reviews.clone(),
        }
    }
}

/// The outcome of one `rigger step`: the WAVE of spawns it newly parked, and whether
/// the run has reached a fixpoint.
///
/// This is exactly what `rigger step` prints as one line of JSON on stdout - the shape
/// the thin native driver reads to spawn the wave's agents in parallel and to decide
/// whether to loop again (§4, spec 04). `wave` serializes as an array of [`WaveItem`]
/// (slim manifests; workers fetch their own prompts via `rigger prompt <id>`); `done`
/// is a plain bool.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Step {
    /// The pending frontier the driver runs now, as slim manifests. Two ready units
    /// with disjoint blast radii park their spawns in the same wave, so fan-out falls
    /// out of the run structure. Ordered deterministically by [`spawn_id`].
    pub wave: Vec<WaveItem>,
    /// True when the run reached a fixpoint: every recorded spawn request already has a
    /// [`SpawnResult`], so the conductor replayed the whole log and parked nothing that
    /// still awaits a courier (all units integrated, or the run terminated). Another
    /// step would change nothing. A non-empty `wave` always implies `done == false`,
    /// since a freshly parked spawn has no result yet.
    pub done: bool,
    /// The halt reason when the run STOPPED on the spawn-budget breaker rather than
    /// converging (Gap 13): e.g. `"budget exhausted: 200/200 spawns"`. `None` on a clean
    /// fixpoint, and OMITTED from the wire then, so a converged run still prints
    /// `{"wave":[],"done":true}` unchanged and a halted one adds `"halted":"..."` - the
    /// `done`/`halted` split the spec (06, Gap 13) calls for. The thin driver treats a
    /// present `halted` as a LOUD stop (a workflow failure carrying the reason), never a
    /// clean completion, so a starved run is never reported as success. Populated by
    /// `rigger step` (`cmd_step`) from the conductor's LIVE breaker state; [`step_result`]
    /// leaves it `None` because a halt is a runtime condition of the current run process,
    /// not derivable from the append-only log alone - a resume with a raised budget clears
    /// it, yet the earlier halt's `BudgetExhausted` event stays in the log.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub halted: Option<String>,
    /// The units that ESCALATED - each exhausted remediation and went terminal WITHOUT
    /// integrating (§4.6, spec 19c unit 1). Empty on a clean run, and OMITTED from the wire
    /// then, so a converged run still prints `{"wave":[],"done":true}` unchanged and a wedged
    /// terminus adds `"escalated":["<unit>",...]`. The thin driver treats a `done` fixpoint
    /// reached with a NON-EMPTY escalated set as a LOUD stop (a workflow failure naming the
    /// units), never a clean completion - so a unit that can never pass review no longer
    /// masquerades as success (a wedged terminus is otherwise indistinguishable from a clean
    /// one). Like [`halted`](Step::halted) this is stamped by `rigger step` (`cmd_step`) from
    /// the conductor's projected [`ledger::RunState::escalated_units`], not by the pure
    /// [`step_result`] log seam (which reasons only about spawn requests/results, not the
    /// unit lifecycle). Lexically ordered for a deterministic wire.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub escalated: Vec<String>,
    /// The push-side anomalies THIS step surfaced (spec 69: the watching discipline's step
    /// wire) - unit ESCALATED, run HALTED, a worker's death RECURRED, the budget crossed
    /// into its final tenth, and STALLED FRONTIER. Empty on a clean step, and OMITTED from
    /// the wire then, so a converged run still prints `{"wave":[],"done":true}` unchanged;
    /// when non-empty a later criterion's driver renders one narrator line per entry naming
    /// its event, unit, and response skill. Like [`escalated`](Step::escalated) this is
    /// stamped by `rigger step` (`cmd_step`) from the conductor's live
    /// [`ledger::RunState::attention`](crate::ledger::RunState::attention) - a fact of THIS
    /// call's own before/after transition, not derivable from the log alone - so the pure
    /// [`step_result`] log seam leaves it empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attention: Vec<AttentionEntry>,
}

/// Compute the [`Step`] a step process prints, from the run stream `events`.
///
/// This is the pure core of `rigger step`, extracted so the wave/done contract is
/// testable without a config, a repo, or the CLI: the command drives the conductor
/// with the replay driver, reads the stream, and delegates here.
///
/// - `wave` is the FULL PENDING FRONTIER: every recorded request with no recorded
///   [`SpawnResult`] - never just the spawns the current process newly parked. A step
///   process killed after parking but before printing (or a driver that died between
///   steps) orphans nothing this way: the next step re-prints every unanswered spawn,
///   so re-running `rigger step` is idempotent and a relaunched driver resumes the
///   in-flight wave. Spawns the driver already ran never reappear: their results are
///   recorded (by the worker itself or its death courier) before the driver steps
///   again. Ordered by [`spawn_id`] (the [`recorded`] map is keyed by id).
/// - `done` is true when no recorded request still awaits a result: every parked spawn
///   has a matching [`SpawnResult`]. An empty log is vacuously done (nothing to run).
pub fn step_result(events: &[Event]) -> Result<Step, serde_json::Error> {
    let recorded = recorded(events)?;
    // The ids a courier has already drained (a recorded result). Folded once, so the
    // wave filter and `done` are O(events) rather than a per-request rescan.
    let mut answered: BTreeSet<String> = BTreeSet::new();
    for e in events {
        if e.type_ == TYPE_SPAWN_RESULT {
            answered.insert(SpawnResult::from_event(e)?.id);
        }
    }
    let wave = recorded
        .values()
        .filter(|req| !answered.contains(&req.id))
        .map(WaveItem::from)
        .collect();
    let done = recorded.keys().all(|id| answered.contains(id));
    // A halt is a RUNTIME condition of the live run (the conductor's in-process breaker),
    // not a fact of the append-only log: a resume with a raised budget clears it while the
    // earlier `BudgetExhausted` event remains recorded. So this pure log seam never sets it;
    // `rigger step` stamps `halted` from the conductor's `RunState::budget_halt`.
    Ok(Step {
        wave,
        done,
        halted: None,
        // The escalated set is a fact of the projected UNIT lifecycle, not of the spawn
        // request/result stream this pure seam folds; `rigger step` stamps it from the
        // conductor's `RunState::escalated_units`, so this leaves it empty (like `halted`).
        escalated: Vec::new(),
        // `attention` is a fact of THIS call's own before/after transition (spec 69,
        // criterion 5), not of the recorded spawn stream this pure seam folds; `rigger
        // step` stamps it from the conductor's live `RunState::attention`, so this leaves
        // it empty too (like `halted` and `escalated`).
        attention: Vec::new(),
    })
}

/// The full prompt a worker fetches for its parked spawn: the persona (when the spawn
/// carries one) followed by the task, separated by a `---` line - exactly what the
/// thin driver used to inline into the worker's agent prompt before waves went
/// by-reference. `None` when no spawn request with this id is recorded.
pub fn prompt_for(events: &[Event], id: &str) -> Result<Option<String>, serde_json::Error> {
    let recorded = recorded(events)?;
    Ok(recorded.get(id).map(|req| {
        if req.system_prompt.is_empty() {
            req.prompt.clone()
        } else {
            format!("{}\n\n---\n\n{}", req.system_prompt, req.prompt)
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjudication_parses_the_verdict_line_only_for_an_adjudicator_result() {
        // The single disposition-parse authority (spec 25) both the review-quality metric
        // and the context-graph finding-expiry read: it self-gates on the adjudicator role,
        // and reads the LAST verdict line for the upheld ids and the reject cause.
        let verdict = "some prose the adjudicator wrote\n\
                       {\"verdict\":\"reject\",\"upheld\":[\"f1\",\"f3\"],\"discarded\":[\"f2\"],\"cause\":\"genuine-defect\"}";

        // An adjudicator result parses.
        let adj = SpawnResult::ok(spawn_id("u1", ROLE_ADJUDICATOR, 0), verdict)
            .adjudication()
            .expect("an adjudicator result with a verdict line parses");
        assert_eq!(adj.upheld, vec!["f1".to_string(), "f3".to_string()]);
        // The EXPLICIT discarded array - the disposition the finding-expiry keys on, read
        // independently of `upheld` (never its complement).
        assert_eq!(adj.discarded, vec!["f2".to_string()]);
        assert_eq!(adj.cause.as_deref(), Some("genuine-defect"));
        // The raw `verdict` literal itself (spec 94 c3, THE POSITION MODEL: `scrub_track`'s
        // own red/green mark colour reads this, never re-deriving approve/reject from
        // `cause`'s presence, which is silent on a reject that declared none).
        assert_eq!(adj.verdict.as_deref(), Some("reject"));

        // The SAME output on a NON-adjudicator (lens) result yields None - only an
        // adjudicator disposes findings, even if a lens echoed a verdict-shaped line.
        assert!(
            SpawnResult::ok(spawn_id("u1", &lens_role("sdet"), 0), verdict)
                .adjudication()
                .is_none(),
            "a non-adjudicator result never disposes findings"
        );

        // An adjudicator result with no verdict line (old-contract / prose only) is None.
        assert!(
            SpawnResult::ok(spawn_id("u1", ROLE_ADJUDICATOR, 0), "just prose, no json")
                .adjudication()
                .is_none(),
            "an adjudicator result with no verdict line disposes nothing"
        );

        // An approve verdict carries upheld ids but no cause.
        let approve = SpawnResult::ok(
            spawn_id("u1", ROLE_ADJUDICATOR, 0),
            r#"{"verdict":"approve","upheld":["a1"]}"#,
        )
        .adjudication()
        .expect("an approve verdict parses");
        assert_eq!(approve.upheld, vec!["a1".to_string()]);
        // No `discarded` array on this verdict -> an empty discard set, NOT the complement
        // of `upheld`; an approve that upholds one finding discards nothing.
        assert_eq!(approve.discarded, Vec::<String>::new());
        assert_eq!(approve.cause, None);
        assert_eq!(approve.verdict.as_deref(), Some("approve"));
    }

    #[test]
    fn the_sdet_author_role_token_is_a_distinct_first_class_role() {
        // Spec 32 c1: the SDET periphery-test AUTHOR is its OWN role, spawned at the build
        // seam, separate from the implementer and every reviewer role. Its token names a
        // distinct spawn so the conductor (the spawn-seam unit) can park it independently.
        assert_eq!(ROLE_SDET_AUTHOR, "sdet-author");
        for other in [ROLE_IMPLEMENTER, ROLE_ADVERSARY, ROLE_ADJUDICATOR] {
            assert_ne!(
                ROLE_SDET_AUTHOR, other,
                "the sdet-author is a distinct role, not the implementer or a reviewer"
            );
        }
        // It rounds through the spawn-id vocabulary like any other role: a spawn id built
        // with it recovers it via `spawn_role`.
        let id = spawn_id("u1", ROLE_SDET_AUTHOR, 0);
        assert_eq!(id, "u1/sdet-author#0");
        assert_eq!(spawn_role(&id), ROLE_SDET_AUTHOR);
        // ...and it is NOT the read-only `sdet` review LENS role (which authors nothing).
        assert_ne!(
            ROLE_SDET_AUTHOR,
            lens_role("sdet"),
            "the write-capable author role is distinct from the read-only sdet review lens"
        );
    }

    #[test]
    fn spawn_id_is_a_pure_deterministic_function_of_the_triple() {
        // Same coordinates -> same id, every time (no wall clock, no counter).
        assert_eq!(
            spawn_id("spawn-req", ROLE_IMPLEMENTER, 0),
            spawn_id("spawn-req", ROLE_IMPLEMENTER, 0)
        );
        assert_eq!(spawn_id("u", ROLE_IMPLEMENTER, 0), "u/implementer#0");
    }

    #[test]
    fn spawn_id_varies_on_every_coordinate() {
        // Each of unit, role, and attempt independently changes the id, so distinct
        // spawns never collide onto one id (which would cross-wire their results).
        let base = spawn_id("u", ROLE_IMPLEMENTER, 0);
        assert_ne!(
            base,
            spawn_id("v", ROLE_IMPLEMENTER, 0),
            "unit must vary the id"
        );
        assert_ne!(
            base,
            spawn_id("u", ROLE_ADVERSARY, 0),
            "role must vary the id"
        );
        assert_ne!(
            base,
            spawn_id("u", ROLE_IMPLEMENTER, 1),
            "attempt must vary the id"
        );
    }

    #[test]
    fn spawn_retry_id_zero_is_the_plain_spawn_id() {
        // The reviewer's ORIGINAL spawn (retry ordinal 0) keeps the exact base id, so
        // the normal non-degenerate path is byte-identical to pre-Gap-18 behavior.
        for role in [ROLE_ADVERSARY, ROLE_ADJUDICATOR, ROLE_IMPLEMENTER] {
            for attempt in [0, 1, 4] {
                assert_eq!(
                    spawn_retry_id("u", role, attempt, 0),
                    spawn_id("u", role, attempt),
                    "retry ordinal 0 must equal the plain spawn id"
                );
            }
        }
        assert_eq!(
            spawn_retry_id("u", &lens_role("sdet"), 2, 0),
            spawn_id("u", &lens_role("sdet"), 2)
        );
    }

    #[test]
    fn spawn_retry_id_is_a_pure_deterministic_function_of_the_four_coordinates() {
        // Gap 18 replay-safety: two step processes replaying the same history must
        // compute the identical retry id (no wall clock, no randomness), so a recorded
        // degenerate-respawn result matches the call that produced it across processes.
        assert_eq!(
            spawn_retry_id("u", ROLE_ADJUDICATOR, 1, 2),
            spawn_retry_id("u", ROLE_ADJUDICATOR, 1, 2)
        );
        assert_eq!(
            spawn_retry_id("u", ROLE_ADJUDICATOR, 1, 2),
            "u/adjudicator#1~retry2"
        );
    }

    #[test]
    fn spawn_retry_id_varies_on_the_retry_ordinal() {
        // Each respawn ordinal produces a DISTINCT id, so the original spawn and every
        // respawn are individually addressable (their results never cross-wire) and a
        // stepwise/replay driver parks and answers each independently.
        let base = spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 0);
        let r1 = spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 1);
        let r2 = spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 2);
        assert_ne!(base, r1, "retry 1 must differ from the original spawn");
        assert_ne!(r1, r2, "each respawn ordinal must vary the id");
        assert_ne!(base, r2);
        // The retry suffix carries no `/` or `#` collision with the id structure.
        for id in [&r1, &r2] {
            assert!(id.starts_with(&base), "a respawn extends its base id: {id}");
        }
    }

    #[test]
    fn lens_ids_disambiguate_parallel_lenses() {
        // Two lenses on the same unit+attempt get distinct ids off their agent ids,
        // so a fan-out review's parallel spawns are individually addressable.
        assert_ne!(
            spawn_id("u", &lens_role("sdet"), 0),
            spawn_id("u", &lens_role("architect"), 0)
        );
        assert_eq!(spawn_id("u", &lens_role("sdet"), 0), "u/lens:sdet#0");
    }

    #[test]
    fn a_request_round_trips_through_its_event() {
        let req = SpawnRequest {
            system_prompt: "persona".into(),
            model: "sonnet".into(),
            tools: vec!["Read".into()],
            dir: "/wt".into(),
            blast_radius: vec!["a.rs".into()],
            ..test_request("u", "implement", ROLE_IMPLEMENTER, 0, "prompt")
        };

        let ev = req.to_event().unwrap();
        assert_eq!(ev.type_, TYPE_SPAWN_REQUESTED);
        assert_eq!(SpawnRequest::from_event(&ev).unwrap(), req);
    }

    #[test]
    fn empty_optional_fields_are_omitted_from_the_wire() {
        // A minimal request serializes to only the always-present fields, so a
        // persisted spawn event and a printed wave stay compact.
        let req = test_request("u", "implement", ROLE_IMPLEMENTER, 0, "prompt");
        let json = serde_json::to_value(&req).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("unit"));
        assert!(obj.contains_key("stage"));
        assert!(obj.contains_key("prompt"));
        assert!(
            !obj.contains_key("system_prompt"),
            "empty persona is omitted"
        );
        assert!(!obj.contains_key("model"), "empty model is omitted");
        assert!(!obj.contains_key("tools"), "empty tools are omitted");
        assert!(!obj.contains_key("dir"), "empty dir is omitted");
        assert!(
            !obj.contains_key("blast_radius"),
            "empty blast-radius is omitted"
        );
    }

    #[test]
    fn recorded_keys_a_hand_built_wave_by_id_and_is_recorded_checks_membership() {
        // The pure fold's own coverage (spec 93, criterion 1): `recorded`/`is_recorded`
        // read a slice of already-serialized events, with no store involved - the store-
        // backed persistence half (`spawn_store::park_in_run`) has its own integration coverage.
        let a = test_request("a", "implement", ROLE_IMPLEMENTER, 0, "a");
        let b = SpawnRequest {
            model: "sonnet".into(),
            blast_radius: vec!["a.rs".into()],
            ..test_request("b", "implement", ROLE_IMPLEMENTER, 0, "b")
        };
        let events = vec![a.to_event().unwrap(), b.to_event().unwrap()];

        let recorded = recorded(&events).unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[&a.id].unit, "a");
        assert_eq!(recorded[&b.id], b);
        assert!(is_recorded(&events, &a.id));
        assert!(is_recorded(&events, &b.id));
        assert!(!is_recorded(&events, "u/implementer#1"));
    }

    #[test]
    fn a_success_result_round_trips_and_omits_empty_fields() {
        let res = SpawnResult::ok("u/implementer#0", "the agent's final message");
        assert!(!res.is_error());

        let json = serde_json::to_value(&res).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("output"));
        assert!(!obj.contains_key("error"), "no error on a success result");
        assert!(!obj.contains_key("meta"), "null meta is omitted");

        let ev = res.to_event().unwrap();
        assert_eq!(ev.type_, TYPE_SPAWN_RESULT);
        assert_eq!(SpawnResult::from_event(&ev).unwrap(), res);
    }

    #[test]
    fn an_error_result_carries_the_failure_and_optional_meta() {
        let res = SpawnResult::failed("u/adjudicator#1", "agent crashed: non-zero exit")
            .with_meta(serde_json::json!({"by": "courier"}));
        assert!(res.is_error());
        assert_eq!(res.error, "agent crashed: non-zero exit");
        assert_eq!(res.meta, serde_json::json!({"by": "courier"}));

        // The failure survives the event round-trip so a step replays it AS a failure.
        let ev = res.to_event().unwrap();
        assert_eq!(SpawnResult::from_event(&ev).unwrap(), res);
    }

    #[test]
    fn liveness_fault_is_a_recognizable_no_charge_result_that_round_trips() {
        let f = SpawnResult::liveness_fault("u/implementer#0", "the agent hung", "infra");
        assert!(
            f.is_error(),
            "a hung spawn's fault carries a describing error"
        );
        assert!(
            f.is_liveness_fault(),
            "it is recognizable as a liveness fault"
        );
        assert_eq!(f.meta_str(META_LIVENESS_CLASS), "infra");
        // A plain success/failure is NOT a liveness fault.
        assert!(!SpawnResult::ok("u/implementer#0", "done").is_liveness_fault());
        assert!(!SpawnResult::failed("u/implementer#0", "boom").is_liveness_fault());
        // Survives the event round-trip so the replay driver recognizes it on a later step.
        let ev = f.to_event().unwrap();
        let back = SpawnResult::from_event(&ev).unwrap();
        assert_eq!(back, f);
        assert!(back.is_liveness_fault());
    }

    #[test]
    fn spawn_request_max_wall_clock_round_trips_and_omits_when_absent() {
        // Absent (the back-compatible common case): omitted from the wire.
        let plain = test_request("u", "implement", ROLE_IMPLEMENTER, 0, "task");
        assert_eq!(plain.max_wall_clock, None);
        let json = serde_json::to_value(&plain).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("max_wall_clock"),
            "an unbounded spawn omits max_wall_clock from the persisted event"
        );
        // Present: persisted and recovered so the sweep reads it off the parked event.
        let mut bounded = test_request("u", "implement", ROLE_IMPLEMENTER, 0, "task");
        bounded.max_wall_clock = Some(1800);
        let ev = bounded.to_event().unwrap();
        assert_eq!(
            SpawnRequest::from_event(&ev).unwrap().max_wall_clock,
            Some(1800)
        );
    }

    #[test]
    fn resolved_model_reads_the_meta_key_the_worker_reports() {
        // spec 05 line 52: the worker reports the resolved model via `rigger result --meta
        // '{"resolved_model": ..}'`; `meta_str(META_RESOLVED_MODEL)` reads exactly that key so the
        // conductor can copy it onto the spawn's unit events.
        let with = SpawnResult::ok("u/implementer#0", "done")
            .with_meta(serde_json::json!({ "resolved_model": "claude-opus-4-8-20260101" }));
        assert_eq!(
            with.meta_str(META_RESOLVED_MODEL),
            "claude-opus-4-8-20260101"
        );

        // No meta, wrong key, or a non-string value each read as empty (then omitted).
        assert_eq!(
            SpawnResult::ok("u/implementer#0", "done").meta_str(META_RESOLVED_MODEL),
            ""
        );
        assert_eq!(
            SpawnResult::ok("u/implementer#0", "done")
                .with_meta(serde_json::json!({ "by": "courier" }))
                .meta_str(META_RESOLVED_MODEL),
            ""
        );
        assert_eq!(
            SpawnResult::ok("u/implementer#0", "done")
                .with_meta(serde_json::json!({ "resolved_model": 7 }))
                .meta_str(META_RESOLVED_MODEL),
            ""
        );
    }

    #[test]
    fn resolved_model_never_reads_a_conflicting_claim_from_the_agents_own_output() {
        // Spec 61 c10 (AUTHORITATIVE MODEL IDENTITY): "a conflicting agent-prose claim
        // never enters the record" - meta_str(META_RESOLVED_MODEL) is sourced EXCLUSIVELY from the
        // structured `meta` object a runner (never the agent itself) attaches, so a
        // model id an agent typed into its own free-text `output` - even one shaped
        // exactly like the real meta payload - can never be mistaken for it.
        let prose_claim = SpawnResult::ok(
            "u/implementer#0",
            r#"done. {"resolved_model":"a-model-i-am-lying-about"}"#,
        );
        assert_eq!(
            prose_claim.meta_str(META_RESOLVED_MODEL),
            "",
            "a claim living only in `output` (agent prose) must never surface as the resolved model"
        );

        // The SAME output, once a runner ALSO attaches the real value via the
        // structured `meta` channel, is honored - and it is the meta value that wins,
        // never the (different) prose claim sitting right next to it in `output`.
        let with_structured_meta = SpawnResult::ok(
            "u/implementer#0",
            r#"done. {"resolved_model":"a-model-i-am-lying-about"}"#,
        )
        .with_meta(serde_json::json!({ "resolved_model": "claude-sonnet-4-9-20260215" }));
        assert_eq!(
            with_structured_meta.meta_str(META_RESOLVED_MODEL),
            "claude-sonnet-4-9-20260215",
            "the structured meta value is authoritative even when output carries a conflicting claim"
        );
    }

    #[test]
    fn result_of_returns_the_latest_matching_result_and_ignores_other_ids() {
        // The pure fold's own coverage (spec 93, criterion 1): `result_of` reads a slice
        // of already-serialized events - later wins, non-matching ids are ignored - with
        // no store involved. The store-backed persistence half (`spawn_store::record_result`
        // / `record_result_if_absent`, including their atomicity guarantees) has its own
        // integration coverage in `spawn_store`.
        let events = vec![
            SpawnResult::failed("u/implementer#0", "flaked")
                .to_event()
                .unwrap(),
            SpawnResult::ok("u/implementer#0", "recovered")
                .to_event()
                .unwrap(),
        ];
        assert!(result_of(&events, "u/implementer#1").unwrap().is_none());
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert!(
            !got.is_error(),
            "the later success supersedes the earlier failure"
        );
        assert_eq!(got.output, "recovered");
    }

    #[test]
    fn a_result_does_not_count_as_a_parked_request() {
        // The request and result halves share the stream but are distinct facts: a
        // result must not make `recorded`/`is_recorded` (which count REQUESTS) match.
        let events = vec![SpawnResult::ok("u/implementer#0", "done")
            .to_event()
            .unwrap()];
        assert!(
            recorded(&events).unwrap().is_empty(),
            "a result is not a request"
        );
        assert!(!is_recorded(&events, "u/implementer#0"));
    }

    #[test]
    fn recorded_ignores_non_spawn_events() {
        // The spawn fold shares the run stream with the ledger; a foreign event type
        // must be skipped, not decoded as a spawn.
        let events = vec![
            Event::new("UnitStarted", br#"{"id":"u"}"#.to_vec()),
            test_request("u", "implement", ROLE_IMPLEMENTER, 0, "do it")
                .to_event()
                .unwrap(),
        ];
        assert_eq!(
            recorded(&events).unwrap().len(),
            1,
            "only the spawn event folds"
        );
    }

    #[test]
    fn step_wave_is_the_full_pending_frontier_never_answered_spawns() {
        // A prior step parked `plan` and it was ANSWERED; this step parks two disjoint
        // units. The wave is every spawn still awaiting a result - the two new ones in
        // deterministic id order - and never the answered `plan`.
        let old = test_request("plan", "plan", ROLE_IMPLEMENTER, 0, "plan it");
        let events = vec![
            old.to_event().unwrap(),
            SpawnResult::ok(&old.id, "planned").to_event().unwrap(),
            test_request("b", "implement", ROLE_IMPLEMENTER, 0, "b")
                .to_event()
                .unwrap(),
            test_request("a", "implement", ROLE_IMPLEMENTER, 0, "a")
                .to_event()
                .unwrap(),
        ];
        let step = step_result(&events).unwrap();

        let ids: Vec<&str> = step.wave.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["a/implementer#0", "b/implementer#0"],
            "the wave is every unanswered spawn, id-ordered, never the answered `plan`"
        );
        assert!(
            !step.done,
            "two spawns have no result yet, so the run is not done"
        );
    }

    #[test]
    fn wave_item_carries_the_units_build_location_beside_its_worktree() {
        // Spec 77 criterion 1 for a driver that cannot set a worker's environment: the wave
        // names the unit's one build location, derived exactly as the gates derive theirs,
        // and omits it for a spawn with no unit worktree.
        let req = SpawnRequest {
            id: "u1/implementer#0".into(),
            dir: "/scratch/rigger-wt-u1".into(),
            ..Default::default()
        };
        let item = WaveItem::from(&req);
        assert_eq!(
            item.cargo_target_dir.as_deref(),
            Some("/scratch/cargo-target-u1"),
            "the worktree's cargo-target sibling, the gates' own directory"
        );
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            json.contains("\"cargo_target_dir\":\"/scratch/cargo-target-u1\""),
            "the driver reads it off the wave: {json}"
        );
        let bare = SpawnRequest {
            id: "plan/plan#0".into(),
            ..Default::default()
        };
        let item = WaveItem::from(&bare);
        assert!(
            item.cargo_target_dir.is_none(),
            "no unit worktree, no per-unit location"
        );
        let json = serde_json::to_value(&item).unwrap();
        assert!(
            json.get("cargo_target_dir").is_some_and(|v| v.is_null()),
            "an explicit null when absent, so the courier schema can require the key: {json}"
        );
    }

    #[test]
    fn wave_item_marker_path_is_absent_from_a_pure_fold_and_null_on_the_wire_when_unset() {
        // `WaveItem::from` cannot know the scratch root or run id, so it leaves the resolved
        // marker path absent; `rigger step` stamps it. An absent marker path is omitted from
        // the wire (like an unbounded spawn's), so a slim manifest stays slim.
        let req = test_request("u", "implement", ROLE_IMPLEMENTER, 0, "task");
        let item = WaveItem::from(&req);
        assert_eq!(item.marker_path, None, "a pure fold leaves the path unset");
        let json = serde_json::to_value(&item).unwrap();
        assert!(
            json.get("marker_path").is_some_and(|v| v.is_null()),
            "an unstamped marker path is an explicit null on the wire, so the courier schema \
             can require the key: {json}"
        );

        // Once stamped (as cmd_step does from liveness::marker_path), it rides the wire so the
        // thin driver frames the heartbeat + watchdog around the exact path the sweep reads.
        let mut stamped = item;
        stamped.marker_path = Some("/scratch/agent-live/r1/u_implementer_0".into());
        let json = serde_json::to_value(&stamped).unwrap();
        assert_eq!(
            json.get("marker_path").and_then(|v| v.as_str()),
            Some("/scratch/agent-live/r1/u_implementer_0"),
            "a stamped marker path is carried on the wire verbatim"
        );
    }

    #[test]
    fn a_spawn_requests_title_rides_the_wire_and_is_omitted_when_empty() {
        // The live work-line (spec 19a, c4): a spawn carries its unit's criterion as a
        // `title` so the thin driver can narrate the actual WORK, not just `<unit>:<stage>`.
        // An empty title (the back-compatible default) is omitted from the wire so a spawn
        // that carries no criterion serializes exactly as before.
        let titled = SpawnRequest {
            title: "a test over the production render asserts the blocker line".into(),
            ..test_request("u", "u", ROLE_IMPLEMENTER, 0, "task")
        };
        let json = serde_json::to_value(&titled).unwrap();
        assert_eq!(
            json.get("title").and_then(|v| v.as_str()),
            Some("a test over the production render asserts the blocker line"),
            "a set title rides the wire so a wave read off the log carries the work-line"
        );

        // The default leaves the title empty and omits it from the wire:
        // an untitled spawn's SpawnRequested event is byte-for-byte the historical shape.
        let untitled = test_request("u", "u", ROLE_IMPLEMENTER, 0, "task");
        assert!(untitled.title.is_empty(), "the default title is empty");
        let json = serde_json::to_value(&untitled).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("title"),
            "an empty title is omitted from the wire (back-compatible)"
        );
    }

    #[test]
    fn wave_item_copies_the_request_title_so_the_thin_driver_renders_the_work() {
        // The false-green seam (finding adv-u4-fix-is-necessary-and-sufficient): the wave the
        // thin driver actually reads is a `Vec<WaveItem>`, NOT the SpawnRequest, so a title on
        // the request alone renders NOTHING. `WaveItem::from` MUST copy the title, and it must
        // ride the printed wire, or `rigger.js` has nothing to narrate.
        let req = SpawnRequest {
            title: "do the work".into(),
            ..test_request("u", "u", ROLE_IMPLEMENTER, 0, "task")
        };
        let item = WaveItem::from(&req);
        assert_eq!(
            item.title, "do the work",
            "WaveItem::from must copy the request's title - the seam rigger.js reads"
        );
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(
            json.get("title").and_then(|v| v.as_str()),
            Some("do the work"),
            "the title rides the WaveItem wire so the printed wave carries the work-line"
        );

        // An untitled request yields an untitled item, omitted from the slim manifest.
        let bare = WaveItem::from(&test_request("u", "u", ROLE_IMPLEMENTER, 0, "task"));
        assert!(
            bare.title.is_empty(),
            "an untitled request yields an untitled item"
        );
        let json = serde_json::to_value(&bare).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("title"),
            "an empty title is omitted from the slim manifest"
        );
    }

    #[test]
    fn step_wave_carries_the_unit_title_end_to_end() {
        // The regression the false-green demands: assert on the PRINTED Step wave. A parked
        // request carrying a title, folded through `step_result` into the JSON `rigger step`
        // prints, must surface the title on the wave item - the exact wire `rigger.js` reads
        // to narrate the work. This fails if `WaveItem::from` drops the title (the class of
        // bug that shipped a title on the request but rendered nothing).
        let req = SpawnRequest {
            title: "the live work-line shows the actual criterion".into(),
            ..test_request("u", "u", ROLE_IMPLEMENTER, 0, "task")
        };
        let events = vec![req.to_event().unwrap()];
        let step = step_result(&events).unwrap();
        let json = serde_json::to_value(&step).unwrap();
        assert_eq!(
            json["wave"][0]["title"].as_str(),
            Some("the live work-line shows the actual criterion"),
            "the printed Step wave must carry each spawn's title end to end"
        );
    }

    #[test]
    fn the_thin_driver_renders_the_work_line_at_both_sites() {
        // The RENDER contract (spec 19a, c4): the thin driver narrates the unit's criterion at
        // BOTH surfaces `runWorker` controls - the per-worker progress-group LABEL and a log()
        // NARRATOR line - so the live view shows the actual WORK, not just the `{unit}:{stage}`
        // group. Paired with `step_wave_carries_the_unit_title_end_to_end` (which proves the
        // title reaches the PRINTED wave), these pins are NOT the dead source-proxy a WaveItem
        // drop would survive: that wire test catches the drop, and each assertion below pins a
        // DISTINCT render site so neither can be silently deleted without a red test.
        let js = include_str!("../workflows/rigger.js");

        // The work-line is the wave item's title (`req.title`), collapsed to one narration line.
        assert!(
            js.contains("work = (req.title"),
            "the work-line must be sourced from the wave item's title"
        );
        // Site 1 - the progress-group LABEL is the work-line, never bare `req.id`, so the item
        // shown live in the /workflows group names the criterion.
        assert!(
            js.contains("label: workLabel") && js.contains("workLabel = work"),
            "the worker's agent() label must be the work-line label, not bare req.id"
        );
        // Site 2 - a log() NARRATOR announces the work-line (distinct code from the label), so a
        // long silent stretch is a visible stream (spec 14) that names the work.
        assert!(
            js.contains("starting ${req.id}"),
            "a log() narrator line must announce the worker's title (the work-line)"
        );
        // Additive: the progress-GROUP label (phaseOf) is a mechanism SEPARATE from the
        // work-line built here - the work-line enriches the item and narrator, it never reads
        // from or replaces the phase group. phaseOf's own role/stage -> {Plan,Build,Review}
        // mapping (spec 67, criterion 1) is pinned in its own dedicated test, not re-derived
        // here.
        assert!(
            js.contains("const ph = phaseOf(req)") && js.contains("phase: ph"),
            "the worker's progress group must still come from phaseOf(req), a mechanism \
             distinct from the work-line label built here"
        );
    }

    #[test]
    fn a_spawn_requests_reviews_roster_rides_the_wire_and_is_omitted_when_empty() {
        // REVIEW TIERS NAME THEIR TARGETS (spec 67, criterion 4): a review-tier spawn
        // (adversary/adjudicator) carries the unit's routed lens roster it judges, so the
        // thin driver can name it in the action phrase. An empty roster
        // (the back-compatible default - a lens spawn, or an older conductor) is omitted from
        // the wire so a spawn that carries no roster serializes exactly as before.
        let rostered = SpawnRequest {
            reviews: vec!["lens:sdet".into(), "lens:architecture-reviewer".into()],
            ..test_request("u", "u", ROLE_ADVERSARY, 1, "task")
        };
        let json = serde_json::to_value(&rostered).unwrap();
        assert_eq!(
            json.get("reviews").and_then(|v| v.as_array()).cloned(),
            Some(vec![
                Value::String("lens:sdet".into()),
                Value::String("lens:architecture-reviewer".into())
            ]),
            "a set roster rides the wire so a wave read off the log carries it"
        );

        // The default leaves the roster empty and omits it from the wire: a
        // roster-less spawn's SpawnRequested event is byte-for-byte the historical shape.
        let unrostered = test_request("u", "u", ROLE_ADVERSARY, 1, "task");
        assert!(
            unrostered.reviews.is_empty(),
            "the default reviews roster is empty"
        );
        let json = serde_json::to_value(&unrostered).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("reviews"),
            "an empty roster is omitted from the wire (back-compatible)"
        );
    }

    #[test]
    fn wave_item_copies_the_request_reviews_roster() {
        // The same false-green seam `title` closed above: the wave the thin driver actually
        // reads is a `Vec<WaveItem>`, NOT the SpawnRequest, so a roster on the request alone
        // renders NOTHING. `WaveItem::from` must copy the roster, and it must ride the printed
        // wire, or `rigger.js` has nothing to render inside the action phrase.
        let req = SpawnRequest {
            reviews: vec!["lens:sdet".into(), "adversary".into()],
            ..test_request("u", "u", ROLE_ADJUDICATOR, 0, "task")
        };
        let item = WaveItem::from(&req);
        assert_eq!(
            item.reviews,
            vec!["lens:sdet".to_string(), "adversary".to_string()],
            "WaveItem::from must copy the request's roster - the seam rigger.js reads"
        );
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(
            json.get("reviews").and_then(|v| v.as_array()).cloned(),
            Some(vec![
                Value::String("lens:sdet".into()),
                Value::String("adversary".into())
            ]),
            "the roster rides the WaveItem wire so the printed wave carries it"
        );

        // A roster-less request yields a roster-less item, omitted from the slim manifest -
        // the "an older conductor" case the driver must render gracefully.
        let bare = WaveItem::from(&test_request("u", "u", ROLE_ADJUDICATOR, 0, "task"));
        assert!(
            bare.reviews.is_empty(),
            "a roster-less request yields a roster-less item"
        );
        let json = serde_json::to_value(&bare).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("reviews"),
            "an empty roster is omitted from the slim manifest"
        );
    }

    #[test]
    fn step_wave_carries_the_reviews_roster_end_to_end() {
        // The regression the title's false-green already demanded a guard for, mirrored here:
        // assert on the PRINTED Step wave. A parked request carrying a roster, folded through
        // `step_result` into the JSON `rigger step` prints, must surface the roster on the wave
        // item - the exact wire `rigger.js` reads to render it inside the action phrase.
        let req = SpawnRequest {
            reviews: vec!["lens:sdet".into()],
            ..test_request("u", "u", ROLE_ADVERSARY, 0, "task")
        };
        let events = vec![req.to_event().unwrap()];
        let step = step_result(&events).unwrap();
        let json = serde_json::to_value(&step).unwrap();
        assert_eq!(
            json["wave"][0]["reviews"].as_array().cloned(),
            Some(vec![Value::String("lens:sdet".into())]),
            "the printed Step wave must carry each spawn's reviews roster end to end"
        );
    }

    #[test]
    fn step_rerun_reprints_unanswered_spawns_so_a_killed_step_orphans_nothing() {
        // Disposable step processes (spec 04): a step killed after parking but before
        // printing must not orphan its spawns. A later step's wave re-prints every
        // spawn still awaiting a result, so a relaunched driver resumes the in-flight
        // wave; the answered spawn does not reappear.
        let a = test_request("a", "implement", ROLE_IMPLEMENTER, 0, "a");
        let b = test_request("b", "implement", ROLE_IMPLEMENTER, 0, "b");

        // `a` was answered; `b`'s wave JSON never reached a driver (killed step).
        let events = vec![
            a.to_event().unwrap(),
            b.to_event().unwrap(),
            SpawnResult::ok(&a.id, "did a").to_event().unwrap(),
        ];
        let step = step_result(&events).unwrap();
        let ids: Vec<&str> = step.wave.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["b/implementer#0"],
            "the re-run re-prints the unanswered spawn and only it"
        );
        assert!(
            !step.done,
            "b still awaits a result, so the run is not done"
        );

        // Once `b` is answered too, the run has reached a fixpoint with an empty wave.
        let mut events = events;
        events.push(SpawnResult::ok(&b.id, "did b").to_event().unwrap());
        let step = step_result(&events).unwrap();
        assert!(step.wave.is_empty(), "nothing awaits a result");
        assert!(step.done, "every recorded spawn now has a result");
    }

    #[test]
    fn step_on_an_empty_log_is_done_with_an_empty_wave() {
        // No spawn was ever parked: vacuously done, empty wave (nothing left to run).
        let step = step_result(&[]).unwrap();
        assert!(step.wave.is_empty());
        assert!(step.done);
    }

    #[test]
    fn step_serializes_to_a_wave_array_and_a_done_bool() {
        // The JSON `rigger step` prints: {"wave":[<SpawnRequest>...],"done":<bool>}.
        let events = vec![test_request("u", "implement", ROLE_IMPLEMENTER, 0, "do it")
            .to_event()
            .unwrap()];
        let step = step_result(&events).unwrap();

        let json = serde_json::to_value(&step).unwrap();
        let obj = json.as_object().unwrap();
        assert_eq!(obj["wave"].as_array().unwrap().len(), 1);
        assert_eq!(obj["wave"][0]["id"], "u/implementer#0");
        assert_eq!(obj["done"], serde_json::json!(false));
        // Spawn-by-reference: the wave is a SLIM manifest - the prompt and persona
        // never transit the courier relay (they can be hundreds of KB; the worker
        // fetches them from the log via `rigger prompt <id>` / spawn::prompt_for).
        assert!(
            obj["wave"][0].get("prompt").is_none() && obj["wave"][0].get("system_prompt").is_none(),
            "wave items must not carry the prompt or persona"
        );
        // A step_result-produced Step is never a halt: the pure log seam does not know the
        // live breaker state, so the `halted` key is absent from the wire.
        assert!(
            obj.get("halted").is_none(),
            "step_result output must omit the halted field"
        );
    }

    #[test]
    fn step_result_leaves_the_halt_reason_unset() {
        // Gap 13: a halt is a RUNTIME condition of the live run, stamped by `rigger step`
        // from the conductor's in-process breaker - the pure log seam never sets it.
        let events = vec![test_request("u", "implement", ROLE_IMPLEMENTER, 0, "do it")
            .to_event()
            .unwrap()];
        assert_eq!(step_result(&events).unwrap().halted, None);
    }

    #[test]
    fn a_halted_step_serializes_the_reason_and_a_converged_one_omits_it() {
        // Gap 13: the `done`/`halted` split. A halted step carries the reason on the wire
        // so the thin driver can stop loudly; a converged step omits the field entirely,
        // leaving the historical `{"wave":[],"done":true}` shape byte-for-byte unchanged.
        let halted = Step {
            wave: Vec::new(),
            done: true,
            halted: Some("budget exhausted: 2/2 spawns".into()),
            escalated: Vec::new(),
            attention: Vec::new(),
        };
        let obj = serde_json::to_value(&halted).unwrap();
        assert_eq!(obj["done"], serde_json::json!(true));
        assert_eq!(
            obj["halted"],
            serde_json::json!("budget exhausted: 2/2 spawns")
        );

        let converged = Step {
            wave: Vec::new(),
            done: true,
            halted: None,
            escalated: Vec::new(),
            attention: Vec::new(),
        };
        let wire = serde_json::to_string(&converged).unwrap();
        assert_eq!(
            wire, r#"{"wave":[],"done":true}"#,
            "a converged step omits `halted` AND `escalated`, preserving the historical wire shape"
        );
    }

    #[test]
    fn an_escalated_step_serializes_the_set_and_a_clean_one_omits_it() {
        // Spec 19c unit 1: the `done`/escalated split, mirroring the `halted` one. A fixpoint
        // reached with an escalated unit carries the set on the wire so the thin driver stops
        // loudly on a wedged terminus; a clean fixpoint omits the field entirely, leaving the
        // historical `{"wave":[],"done":true}` shape byte-for-byte unchanged.
        let wedged = Step {
            wave: Vec::new(),
            done: true,
            halted: None,
            escalated: vec!["u-a".into(), "u-b".into()],
            attention: Vec::new(),
        };
        let obj = serde_json::to_value(&wedged).unwrap();
        assert_eq!(obj["done"], serde_json::json!(true));
        assert_eq!(obj["escalated"], serde_json::json!(["u-a", "u-b"]));

        let clean = Step {
            wave: Vec::new(),
            done: true,
            halted: None,
            escalated: Vec::new(),
            attention: Vec::new(),
        };
        let wire = serde_json::to_string(&clean).unwrap();
        assert_eq!(
            wire, r#"{"wave":[],"done":true}"#,
            "a clean fixpoint omits `escalated`, preserving the historical wire shape"
        );
    }

    #[test]
    fn an_attention_bearing_step_serializes_the_array_and_a_clean_one_omits_it() {
        // Spec 69, criterion 5: the `done`/`attention` split, mirroring `halted` and
        // `escalated`. A step during which a signal crossed carries the array on the wire;
        // a clean step omits the field entirely, leaving the historical `{"wave":[],
        // "done":true}` shape byte-for-byte unchanged.
        let flagged = Step {
            wave: Vec::new(),
            done: true,
            halted: None,
            escalated: Vec::new(),
            attention: vec![AttentionEntry::unit_scoped(
                crate::ledger::ATTENTION_ESCALATED,
                "u-a",
                "escalated after exhausting remediation",
            )],
        };
        let obj = serde_json::to_value(&flagged).unwrap();
        assert_eq!(
            obj["attention"],
            serde_json::json!([{
                "kind": "escalated",
                "unit": "u-a",
                "detail": "escalated after exhausting remediation",
            }])
        );

        let clean = Step {
            wave: Vec::new(),
            done: true,
            halted: None,
            escalated: Vec::new(),
            attention: Vec::new(),
        };
        let wire = serde_json::to_string(&clean).unwrap();
        assert_eq!(
            wire, r#"{"wave":[],"done":true}"#,
            "a clean step omits `attention`, preserving the historical wire shape"
        );
    }

    #[test]
    fn prompt_for_returns_persona_and_task_by_spawn_id() {
        let mut req = test_request("u", "implement", ROLE_IMPLEMENTER, 0, "do the task");
        req.system_prompt = "you are the implementer".into();
        let events = vec![req.to_event().unwrap()];
        assert_eq!(
            prompt_for(&events, &req.id).unwrap().unwrap(),
            "you are the implementer\n\n---\n\ndo the task",
            "persona above a --- line, then the task"
        );
        assert!(
            prompt_for(&events, "nope/implementer#0").unwrap().is_none(),
            "an unknown id yields None"
        );
    }
}
