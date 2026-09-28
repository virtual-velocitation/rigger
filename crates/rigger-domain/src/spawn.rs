//! The pure half of spawn requests: the deterministic spawn-id scheme, the role tokens, and the
//! request/result/wave-item value types.

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
    unit_sibling(worktree_dir, UNIT_CACHE_PREFIX)
}

/// The dir named `<prefix><slug>` that is a SIBLING of the unit worktree at `worktree_dir`
/// (`<root>/rigger-wt-<slug>` -> `<root>/<prefix><slug>`), or `None` for any dir that is
/// not a unit worktree. The ONE derivation behind [`unit_cache_sibling`] and the per-unit
/// mutants root (`unit_sibling(dir, UNIT_MUTANTS_PREFIX)`, spec 91: exported to the
/// `checkin` stage's `mutation` gate command as `$MUTANTS`, mirroring how the cache sibling
/// is exported as `CARGO_TARGET_DIR`). Pure path arithmetic.
pub fn unit_sibling(worktree_dir: &str, prefix: &str) -> Option<String> {
    let path = std::path::Path::new(worktree_dir);
    let slug = path
        .file_name()?
        .to_str()?
        .strip_prefix(UNIT_WORKTREE_PREFIX)?;
    let parent = path.parent()?.to_str()?;
    Some(format!("{parent}/{prefix}{slug}"))
}

/// Filesystem prefix of a unit's per-unit mutants-root dir (`cargo-mutants-<slug>`), a
/// SIBLING of its worktree under the scratch root (spec 91, THE GATE ENVIRONMENT) - the
/// exact same sibling shape as [`UNIT_CACHE_PREFIX`]'s `cargo-target-<slug>`. See
/// [`UNIT_WORKTREE_PREFIX`]'s doc for why this lives here rather than in `worktree`.
pub const UNIT_MUTANTS_PREFIX: &str = "cargo-mutants-";

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

/// A spawn-protocol record that travels as the whole JSON body of ONE event type:
/// [`SpawnRequest`] as [`TYPE_SPAWN_REQUESTED`], [`SpawnResult`] as [`TYPE_SPAWN_RESULT`].
/// Implementing it is all a record needs to get its [`SpawnEvent`] conversions.
pub trait SpawnEventBody: Serialize + serde::de::DeserializeOwned {
    /// The event type this record is appended to the run stream as.
    const EVENT_TYPE: &'static str;
}

impl SpawnEventBody for SpawnRequest {
    const EVENT_TYPE: &'static str = TYPE_SPAWN_REQUESTED;
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
    /// The one constructor every named result below is a thin variant over: `output` on a
    /// success, `error` on a failure, and no courier metadata.
    fn outcome(id: String, output: String, error: String) -> SpawnResult {
        SpawnResult {
            id,
            output,
            error,
            meta: Value::Null,
        }
    }

    /// A SUCCESSFUL result: the agent finished and produced `output`.
    pub fn ok(id: impl Into<String>, output: impl Into<String>) -> SpawnResult {
        Self::outcome(id.into(), output.into(), String::new())
    }

    /// A FAILED result (`rigger result --error`): the spawn errored with `error`. The
    /// replay driver answers a recorded failure with a driver error, never a fake
    /// success.
    pub fn failed(id: impl Into<String>, error: impl Into<String>) -> SpawnResult {
        Self::outcome(id.into(), String::new(), error.into())
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
        Self::failed(id, error).with_meta(serde_json::json!({ META_LIVENESS_CLASS: class }))
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
}

impl SpawnEventBody for SpawnResult {
    const EVENT_TYPE: &'static str = TYPE_SPAWN_RESULT;
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

/// A [`SpawnEventBody`] record's conversions to and from its event, defined ONCE for every
/// such record by the blanket impl below.
pub trait SpawnEvent: Sized {
    /// Serialize this record as its event, ready to append to the run stream.
    fn to_event(&self) -> Result<Event, serde_json::Error>;

    /// Recover a record from its event body.
    fn from_event(e: &Event) -> Result<Self, serde_json::Error>;
}

impl<T: SpawnEventBody> SpawnEvent for T {
    fn to_event(&self) -> Result<Event, serde_json::Error> {
        Ok(Event::new(T::EVENT_TYPE, serde_json::to_vec(self)?))
    }

    fn from_event(e: &Event) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&e.data)
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

/// A minimal request for the crate's unit tests, defined once with the shared fixtures.
#[cfg(test)]
pub(crate) use crate::test_support::test_request;
