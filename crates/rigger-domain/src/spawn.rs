//! The pure half of spawn requests: the deterministic spawn-id scheme, the role tokens, and the
//! request/result/wave-item value types.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::eventstore::Event;
use crate::ledger::RequiredItem;

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
/// not a unit worktree. The ONE derivation behind [`unit_cache_sibling`] and the per-unit gate
/// scratch root (`unit_sibling(dir, UNIT_GATE_SCRATCH_PREFIX)`, spec 113: handed to every gate
/// that runs for the unit as `RIGGER_GATE_SCRATCH`, mirroring how the cache sibling is handed
/// as `CARGO_TARGET_DIR`). Pure path arithmetic.
pub fn unit_sibling(worktree_dir: &str, prefix: &str) -> Option<String> {
    let path = std::path::Path::new(worktree_dir);
    let slug = path
        .file_name()?
        .to_str()?
        .strip_prefix(UNIT_WORKTREE_PREFIX)?;
    let parent = path.parent()?.to_str()?;
    Some(format!("{parent}/{prefix}{slug}"))
}

/// Filesystem prefix of a unit's gate scratch root (`rigger-gate-<slug>`), a SIBLING of its
/// worktree under the scratch root (spec 113, THE GATE SCRATCH ROOT IS HANDED GENERICALLY):
/// every gate that runs for the unit is handed it as `RIGGER_GATE_SCRATCH`, a directory the
/// unit's gates share and no other unit's gate sees. rigger never creates it; a gate that uses
/// it does. See [`UNIT_WORKTREE_PREFIX`]'s doc for why this lives here rather than in
/// `worktree`.
pub const UNIT_GATE_SCRATCH_PREFIX: &str = "rigger-gate-";

/// Filesystem prefix of a unit's REVIEW build cache (`review-target-<slug>`), a SIBLING of its
/// worktree under the scratch root: the `CARGO_TARGET_DIR` every review tier of the unit is
/// spawned with, so a reviewer's own reproduction - in its scratch worktree, at whatever sha it
/// probes - never builds into, or swaps a binary in, the unit's own cache that the implementer,
/// the sdet-author and the gates share. See [`UNIT_WORKTREE_PREFIX`]'s doc for why this lives
/// here rather than in `worktree`.
pub const UNIT_REVIEW_CACHE_PREFIX: &str = "review-target-";

/// The unit slug of a scratch-root entry that is one of a unit's per-unit caches - its build
/// cache (`cargo-target-<slug>`), its review build cache (`review-target-<slug>`) or its gate
/// scratch root (`rigger-gate-<slug>`) - or `None` for any other name. The slug may be empty
/// (a bare `cargo-target-`). Every scratch walk classifies by this one predicate (spec 113,
/// THE GATE SCRATCH ROOT HAS ONE LIFECYCLE), so each is reported, measured and reclaimed
/// exactly as the unit's build cache is.
pub fn unit_scratch_slug(name: &str) -> Option<&str> {
    name.strip_prefix(UNIT_CACHE_PREFIX)
        .or_else(|| name.strip_prefix(UNIT_REVIEW_CACHE_PREFIX))
        .or_else(|| name.strip_prefix(UNIT_GATE_SCRATCH_PREFIX))
}

/// The event type a parked spawn request is persisted as - the "spawn-request" half
/// of the spawn-request/result pair the spec permits as the only new vocabulary parking
/// needs. It is deliberately NOT one of the run-lifecycle events the
/// ledger folds, so an unknown-event-ignoring projection (the ledger, the context
/// graph) skips it and only [`recorded`] and the replay driver read it.
pub const TYPE_SPAWN_REQUESTED: &str = "SpawnRequested";

/// The role token for the unit's implementer (the stage's own `agent`).
pub const ROLE_IMPLEMENTER: &str = "implementer";
/// The role token of a unit review's first adversary step; a later one is
/// [`review_step_role`]'s `adversary@<k>`.
pub const ROLE_ADVERSARY: &str = "adversary";
/// The role token of a unit review's adjudicator step (the gating verdict), of which a panel has
/// one at most.
pub const ROLE_ADJUDICATOR: &str = "adjudicator";
/// The tier label of every lens spawn, whichever step it sits in: what the dash groups under and
/// the metrics and the canary fold and report under. A constant so a `match` can name it; the
/// label is [`ReviewKind::tier`]'s, and the adjudicator's is [`ROLE_ADJUDICATOR`].
pub const TIER_LENS: &str = ReviewKind::Lens.tier();
/// The tier label of every adversary spawn, whichever step it sits in.
pub const TIER_ADVERSARY: &str = ReviewKind::Adversary.tier();
/// The role token for the SDET periphery-test AUTHOR (spec 32): the write-capable role
/// that authors the periphery test layer (contract / API / integration) at the build
/// seam - after the implementer emits green and before the pre-gate commit - so no
/// boundary surface a unit exposes lands untested. A FIRST-CLASS role like the
/// implementer and the reviewers (the token keys its spawn ids whichever agent the workflow
/// declares its test author, `defaults.test_author`, as the adversary/adjudicator tokens key
/// theirs), and DISTINCT from the read-only `sdet` review LENS (`lens:sdet`), which reviews the
/// implementer's code and cannot write.
pub const ROLE_SDET_AUTHOR: &str = "sdet-author";

/// The separator of a review role token's step ordinal: `lens:c@2`, `adversary@2`. It is none of
/// the `/`, `#` and `~` of a spawn id, so [`spawn_role`] and [`unit_of`] read a stepped id as they
/// read any other, and an agent id never holds it.
pub const STEP_TOKEN_SEPARATOR: char = '@';

/// What every lens token opens with, before the lens's agent id.
const LENS_ROLE_PREFIX: &str = "lens:";

/// The role token of a review lens in the first lens step; a later step's is
/// [`review_step_role`]'s. A step runs its lenses in parallel, so the lens's own agent id
/// disambiguates them within one attempt: two lenses on the same unit+attempt get distinct spawn
/// ids because their role tokens differ.
///
/// ```
/// # use rigger_domain::spawn::lens_role;
/// assert_eq!(lens_role("sdet"), "lens:sdet");
/// ```
pub fn lens_role(agent_id: &str) -> String {
    format!("{LENS_ROLE_PREFIX}{agent_id}")
}

/// The kind of a review step: what [`review_step_role`] reads to spell the role token of an agent
/// that step spawns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewKind {
    Lens,
    Adversary,
    Adjudicator,
}

impl ReviewKind {
    /// The tier label this kind's spawns fold and group under, whichever step they sit in.
    ///
    /// ```
    /// # use rigger_domain::spawn::ReviewKind;
    /// assert_eq!(ReviewKind::Lens.tier(), "lens");
    /// assert_eq!(ReviewKind::Adversary.tier(), "adversary");
    /// assert_eq!(ReviewKind::Adjudicator.tier(), "adjudicator");
    /// ```
    pub const fn tier(self) -> &'static str {
        match self {
            ReviewKind::Lens => "lens",
            ReviewKind::Adversary => ROLE_ADVERSARY,
            ReviewKind::Adjudicator => ROLE_ADJUDICATOR,
        }
    }
}

/// The role token of `agent` in the `k`-th step of `kind`, `k` being 1-based and counted among
/// the steps of that kind in [`ReviewPanel::steps`](crate::config::ReviewPanel::steps): the kind's
/// own token, [`lens_role`], [`ROLE_ADVERSARY`] or [`ROLE_ADJUDICATOR`], at `k == 1`, and from
/// `k == 2` that token followed by [`STEP_TOKEN_SEPARATOR`] and `k`. A panel has one adjudicator
/// step at most, so the adjudicator's token never carries `k`. The inverse is
/// [`review_step_of`].
///
/// ```
/// # use rigger_domain::spawn::{review_step_role, ReviewKind};
/// assert_eq!(review_step_role(ReviewKind::Lens, "sdet", 1), "lens:sdet");
/// assert_eq!(review_step_role(ReviewKind::Lens, "sdet", 2), "lens:sdet@2");
/// assert_eq!(review_step_role(ReviewKind::Adversary, "red-team", 3), "adversary@3");
/// assert_eq!(review_step_role(ReviewKind::Adjudicator, "judge", 1), "adjudicator");
/// ```
pub fn review_step_role(kind: ReviewKind, agent: &str, k: u32) -> String {
    let token = match kind {
        ReviewKind::Lens => lens_role(agent),
        ReviewKind::Adversary => ROLE_ADVERSARY.to_string(),
        ReviewKind::Adjudicator => return ROLE_ADJUDICATOR.to_string(),
    };
    if is_later_step(k) {
        format!("{token}{STEP_TOKEN_SEPARATOR}{k}")
    } else {
        token
    }
}

/// Whether the `k`-th step of a kind is marked with its `k`, on a role token and on a dash label
/// alike: step 1 reads bare.
pub fn is_later_step(k: u32) -> bool {
    k >= 2
}

/// The kind and the 1-based step ordinal the review role token `role` spells, the one parser of
/// a [`review_step_role`] token, or `None` for any other role. A bare token is step 1, and an
/// `adjudicator@k` is no token since none is minted. `role` is [`spawn_role`]'s output: a raw id
/// is never passed here, so a `~retry{n}` suffix is already trimmed.
///
/// ```
/// # use rigger_domain::spawn::{review_step_of, ReviewKind};
/// assert_eq!(review_step_of("lens:sdet"), Some((ReviewKind::Lens, 1)));
/// assert_eq!(review_step_of("adversary@2"), Some((ReviewKind::Adversary, 2)));
/// assert_eq!(review_step_of("adversary@1"), None);
/// assert_eq!(review_step_of("adjudicator@2"), None);
/// assert_eq!(review_step_of("implementer"), None);
/// ```
pub fn review_step_of(role: &str) -> Option<(ReviewKind, u32)> {
    let (token, k) = step_token(role)?;
    let kind = if token == ROLE_ADVERSARY {
        ReviewKind::Adversary
    } else if token == ROLE_ADJUDICATOR {
        ReviewKind::Adjudicator
    } else if token.starts_with(LENS_ROLE_PREFIX) {
        ReviewKind::Lens
    } else {
        return None;
    };
    (kind != ReviewKind::Adjudicator || k == 1).then_some((kind, k))
}

/// `role` split at [`STEP_TOKEN_SEPARATOR`] into its token and step ordinal: a role with no
/// separator is step 1, and a suffix [`step_suffix`] refuses is `None`.
fn step_token(role: &str) -> Option<(&str, u32)> {
    match role.split_once(STEP_TOKEN_SEPARATOR) {
        None => Some((role, 1)),
        Some((token, suffix)) => Some((token, step_suffix(suffix)?)),
    }
}

/// The step ordinal a token's `suffix` spells after [`STEP_TOKEN_SEPARATOR`]: an ASCII decimal
/// with no leading zero and no sign, at least 2 and at most `u32::MAX`.
fn step_suffix(suffix: &str) -> Option<u32> {
    if !matches!(suffix.as_bytes().first(), Some(b'1'..=b'9')) {
        return None;
    }
    suffix.parse().ok().filter(|k| is_later_step(*k))
}

/// The agent id the lens token `role` names, bare or stepped, or `None` for any other role and
/// for a step suffix [`review_step_of`] refuses. `role` is [`spawn_role`]'s output.
///
/// ```
/// # use rigger_domain::spawn::lens_agent_of;
/// assert_eq!(lens_agent_of("lens:sdet"), Some("sdet"));
/// assert_eq!(lens_agent_of("lens:sdet@2"), Some("sdet"));
/// assert_eq!(lens_agent_of("lens:sdet@1"), None);
/// assert_eq!(lens_agent_of("adversary@2"), None);
/// ```
pub fn lens_agent_of(role: &str) -> Option<&str> {
    step_token(role)?.0.strip_prefix(LENS_ROLE_PREFIX)
}

/// Derive a spawn's DETERMINISTIC id from its position in the run structure: the
/// `unit` id, the stage/`role` token, and the 0-based remediation `attempt`.
///
/// The id is a PURE function of these three coordinates - no wall clock, no
/// randomness, no in-memory counter - so two driver processes replaying the same
/// recorded history compute the identical id for the identical spawn, which is what
/// lets a recorded result be matched back to the call that produced it across
/// processes (§4, spec 04).
///
/// A stage produces at most one spawn per role per attempt, each step of a review holding its
/// own [`review_step_role`] token, so the triple `(unit, role, attempt)` names a spawn uniquely.
/// The id is kept human-readable (rather than an opaque hash) because it is the handle an operator
/// reads in `rigger status` and the dash. Unit ids and role tokens are drawn from the run structure
/// (kebab identifiers and the role tokens above), neither of which contains the `/` or `#`
/// separators, so the rendering is unambiguous.
///
/// ```
/// # use rigger_domain::spawn::{spawn_id, lens_role, ROLE_IMPLEMENTER};
/// assert_eq!(spawn_id("spawn-req", ROLE_IMPLEMENTER, 0), "spawn-req/implementer#0");
/// assert_eq!(spawn_id("spawn-req", &lens_role("sdet"), 2), "spawn-req/lens:sdet#2");
/// ```
pub fn spawn_id(unit: &str, role: &str, attempt: u32) -> String {
    format!("{unit}/{role}#{attempt}")
}

/// The metadata key carrying the id of the spawn that emitted an event - a [`spawn_id`], or its
/// respawn form. Defined beside the id grammar so every emit surface, and the one emit guard they
/// all run, read the key the conductor stamps without naming the conductor, which re-exports it.
pub const META_SPAWN: &str = "spawn";

/// Derive the spawn id for a reviewer RESPAWN (Gap 18, spec 07). A reviewer spawn
/// (lens, adversary, or adjudicator) whose recorded result is empty or whitespace-only
/// is an INFRASTRUCTURE fault, not a verdict, so the conductor respawns the SAME
/// reviewer under a NEW deterministic id. `retry` is the 0-based respawn ordinal:
/// `retry == 0` is the reviewer's ORIGINAL spawn and returns the plain [`spawn_id`]
/// unchanged (so the normal, non-degenerate path keeps its exact id and nothing else
/// moves), while each `retry > 0` appends a deterministic `~retry{n}` suffix - `~` is
/// neither the `/` nor the `#` the id structure reserves, so the respawn id stays
/// unambiguous and as human-readable as the original. Each respawn thus gets a DISTINCT id the
/// replay driver parks and answers independently of the original.
///
/// Like [`spawn_id`] it is a PURE function of its coordinates - unit + role + attempt +
/// retry ordinal - with no wall clock, no randomness, and no in-memory counter, so two
/// driver processes replaying the same recorded history compute the identical retry id
/// (the spec 07 Gap-18 replay-safety constraint: every new spawn id derives
/// deterministically from unit + role + attempt + retry ordinal).
///
/// ```
/// # use rigger_domain::spawn::{spawn_retry_id, spawn_id, ROLE_ADJUDICATOR};
/// assert_eq!(
///     spawn_retry_id("u", ROLE_ADJUDICATOR, 1, 0),
///     spawn_id("u", ROLE_ADJUDICATOR, 1),
/// );
/// assert_eq!(spawn_retry_id("u", ROLE_ADJUDICATOR, 1, 2), "u/adjudicator#1~retry2");
/// ```
pub fn spawn_retry_id(unit: &str, role: &str, attempt: u32, retry: u32) -> String {
    with_retry(spawn_id(unit, role, attempt), retry)
}

/// Suffix `base` with the deterministic `~retry{n}` RETRY ORDINAL: `retry == 0` returns `base`
/// unchanged, so an original keeps its exact id, and each `retry > 0` appends `~retry{retry}` -
/// `~` is neither the `/` nor the `#` an id's structure reserves. The ONE minting authority for
/// the suffix [`retry_of`] reads back, shared by a respawn's [`spawn_retry_id`] and the gate key
/// an infra rerun records its verdict under, so the two can never spell it apart.
///
/// ```
/// # use rigger_domain::spawn::with_retry;
/// assert_eq!(with_retry("u/gate:test#2".into(), 0), "u/gate:test#2");
/// assert_eq!(with_retry("u/gate:test#2".into(), 1), "u/gate:test#2~retry1");
/// ```
pub fn with_retry(base: String, retry: u32) -> String {
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
/// # use rigger_domain::spawn::speculation_group_id;
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
/// # use rigger_domain::spawn::{spawn_role, ROLE_ADJUDICATOR};
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
/// # use rigger_domain::spawn::unit_of;
/// assert_eq!(unit_of("u1/implementer#0"), Some("u1"));
/// assert_eq!(unit_of("u43-c1-machinery-gone/lens:sdet#1"), Some("u43-c1-machinery-gone"));
/// assert_eq!(unit_of("u1/adjudicator#1~retry2"), Some("u1"));
/// assert_eq!(unit_of("bare-id"), None);
/// ```
pub fn unit_of(id: &str) -> Option<&str> {
    id.rsplit_once('/').map(|(unit, _)| unit)
}

/// The `{unit}/{role}` SEAT of a spawn id `{unit}/{role}#{attempt}` - the `#{attempt}` ordinal and
/// a `~retry{n}` respawn suffix both trimmed - the companion of [`unit_of`] and [`spawn_role`].
/// Every attempt and respawn of one unit's role shares it, so it names a record's author across
/// the remediation attempts that re-emit the record.
///
/// ```
/// # use rigger_domain::spawn::unit_role_of;
/// assert_eq!(unit_role_of("u1/implementer#0"), "u1/implementer");
/// assert_eq!(unit_role_of("u1/adjudicator#1~retry2"), "u1/adjudicator");
/// assert_eq!(unit_role_of("u1/lens:sdet#3"), "u1/lens:sdet");
/// ```
pub fn unit_role_of(id: &str) -> &str {
    id.split(['#', '~']).next().unwrap_or(id)
}

/// The remediation ATTEMPT ordinal of a spawn id `{unit}/{role}#{attempt}` (a `~retry{n}`
/// respawn suffix trimmed first), or `0` when the id carries no `#{attempt}`. The inverse of
/// the `#{attempt}` ordinal [`spawn_id`] mints - kept here beside [`spawn_role`] so the id
/// grammar has ONE owner: a reader never re-parses `#`/`~retry` in a view adapter, which would
/// silently diverge if the separators ever moved with the struct. A gate key's
/// `{gate}#{attempt}~retry{n}` run segment ends in the same tail, so its attempt reads here too.
///
/// ```
/// # use rigger_domain::spawn::attempt_of;
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
/// # use rigger_domain::spawn::retry_of;
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
    /// The items a reject requires the next attempt to fix (its `required` array): what the
    /// next attempt is handed and the next review round holds the unit to. Empty when the
    /// line names none, or names them in any other shape.
    pub required: Vec<RequiredItem>,
}

/// The adjudicator's reject `cause` blaming a gate, tool or harness failure rather than the
/// author's code - the one spelling of the adjudicator persona's `infra-fault` cause that
/// [`Adjudication::is_infra_fault`] reads.
pub const CAUSE_INFRA_FAULT: &str = "infra-fault";

/// The plan-critique adjudicator's reject `cause` for a defect in a criterion's own text that no
/// decomposition can remove (spec 112, criterion 5): the one cause that is a spec defect, which
/// can stop the run, spelled once for the critique prompt that asks for it and for
/// [`Adjudication::is_spec_ambiguity`], which reads it.
pub const CAUSE_SPEC_AMBIGUITY: &str = "spec-ambiguity";

impl Adjudication {
    /// Parse an adjudicator's raw `output` for its grown verdict line (spec 11): the LAST
    /// JSON object line carrying a `verdict`, `upheld`, or `discarded` field yields the upheld
    /// and discarded finding ids and the rejection cause. `None` when the output carries no
    /// verdict line (an old-contract adjudicator, or unparseable output). The single
    /// verdict-line parse every reader shares - [`SpawnResult::adjudication`] for a recorded
    /// result, the conductor for the verdict a review round just returned.
    pub fn parse(output: &str) -> Option<Adjudication> {
        for line in output.lines().rev() {
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
            let required = v
                .get("required")
                .and_then(|r| serde_json::from_value(r.clone()).ok())
                .unwrap_or_default();
            return Some(Adjudication {
                upheld: str_array("upheld"),
                discarded: str_array("discarded"),
                cause,
                verdict,
                required,
            });
        }
        None
    }

    /// Whether the verdict blames infrastructure ([`CAUSE_INFRA_FAULT`]): the review could
    /// not judge the author's code, so the conductor reruns the stage uncharged instead of
    /// handing the author a remediation attempt.
    pub fn is_infra_fault(&self) -> bool {
        self.has_cause(CAUSE_INFRA_FAULT)
    }

    /// Whether the verdict blames the spec ([`CAUSE_SPEC_AMBIGUITY`]): the plan-critique reject
    /// names a defect in a criterion's own text that no re-plan can remove.
    pub fn is_spec_ambiguity(&self) -> bool {
        self.has_cause(CAUSE_SPEC_AMBIGUITY)
    }

    /// Whether the verdict declared exactly `cause`: the one cause comparison every named cause
    /// predicate reads.
    fn has_cause(&self, cause: &str) -> bool {
        self.cause.as_deref() == Some(cause)
    }
}

/// A single spawn request: one agent to run, plus the deterministic id that names it
/// and the display labels its progress is grouped under.
///
/// Persisted as the [`TYPE_SPAWN_REQUESTED`] event body. Empty optional fields are omitted
/// from the wire to keep the persisted event compact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// The deterministic id (see [`spawn_id`]): `{unit}/{role}#{attempt}`.
    pub id: String,
    /// The unit this spawn belongs to - the display label's unit half and the
    /// correlation key the replay driver and budget breaker group spawns under.
    pub unit: String,
    /// The stage that produced this spawn - the display label's stage half.
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
    /// (§5.3), which scopes the tool-boundary injection of peer decisions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blast_radius: Vec<String>,
    /// The per-spawn wall-clock bound in SECONDS (spec 10, unit 3), resolved from the
    /// agent's `max_wall_clock` (its own, or the inherited `defaults.max_wall_clock`).
    /// The driver treats this spawn as a hung/infra fault once its liveness marker has
    /// stayed silent longer than this outside any hold of the run.
    /// `None` (the common back-compatible case) means unbounded - never timed out - and
    /// is omitted from the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_clock: Option<u64>,
    /// The spawn's LIVE WORK-LINE (spec 19a, c4): the unit's criterion (its
    /// [`Stage::coverage`](crate::config::Stage), trimmed), threaded from the conductor so
    /// a driver can narrate the actual WORK a worker is doing - not just its
    /// `{unit}:{stage}` progress-group label. Omitted from the wire when empty (a spawn with no criterion - e.g. a plan/canary stage - serializes
    /// exactly as before), so it is a purely additive, back-compatible field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The routed review roster a review-tier spawn (adversary/adjudicator) judges (spec 67,
    /// criterion 4): the role tokens of the spawns of every review step before it (e.g.
    /// `["lens:sdet", "lens:architecture-reviewer"]` for an adversary after one lens step, those
    /// plus [`ROLE_ADVERSARY`] for the adjudicator after that adversary, and a later step of a
    /// kind under its own token) - stamped by the CONDUCTOR from the actually-routed panel (light
    /// or full), never guessed by the driver, so a replayed or tier-reduced roster is never stale.
    /// Omitted from the wire when empty (a lens spawn, an empty panel, or an older conductor) - a purely additive,
    /// back-compatible field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviews: Vec<String>,
    /// The build location the CONDUCTOR chose for this spawn - the `CARGO_TARGET_DIR` its
    /// spawn environment carries: the unit's `cargo-target-<slug>` cache for a spawn that
    /// builds the unit's tree, the unit's `review-target-<slug>` cache for a review tier.
    /// Recorded so every reader of the spawn names the SAME directory from one choice.
    /// Omitted from the wire when empty (a spawn with no unit worktree, or a request an older
    /// binary recorded).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cargo_target_dir: String,
}

/// The [`TYPE_SPAWN_REQUESTED`] event-meta key, beside the run id's, carrying the scratch root
/// the spawn's liveness marker is written under (spec 101). Where a spawn's marker
/// lives is log-carried state, never a reader's process state: every marker reader resolves it
/// from this recorded root, so a reader whose own environment (`RIGGER_TMPDIR`,
/// `XDG_CACHE_HOME`, `HOME`) resolves a different scratch root still finds a live worker's
/// marker. A request recorded before the root was carried, or parked with no scratch root, has
/// none, and a reader falls back to its own resolution for it.
pub const META_MARKER_ROOT: &str = "marker_root";

/// The scratch root each spawn's request recorded its liveness marker under
/// ([`META_MARKER_ROOT`]), keyed by spawn id; a spawn whose request recorded none is absent. A
/// re-parked id collapses to its last-written request, as in [`recorded`], and a request body
/// with no readable id propagates its parse error, as there. Reads only each request's id, never
/// its prompt.
pub fn marker_roots(events: &[Event]) -> Result<BTreeMap<String, String>, serde_json::Error> {
    #[derive(Deserialize)]
    struct RequestId {
        id: String,
    }
    let mut out = BTreeMap::new();
    for e in spawn_requested_events(events) {
        let id = serde_json::from_slice::<RequestId>(&e.data)?.id;
        match e.meta.get(META_MARKER_ROOT) {
            Some(root) => out.insert(id, root.clone()),
            None => out.remove(&id),
        };
    }
    Ok(out)
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
/// only [`result_of`] and the replay driver read it. The driver that ran the spawn writes one;
/// the replay driver answers an already-recorded spawn by returning the matching
/// result instead of re-running the agent.
pub const TYPE_SPAWN_RESULT: &str = "SpawnResult";

/// The meta key by which a driver reports the RESOLVED model id that actually
/// served its spawn (spec 05 line 52), stored in [`SpawnResult::meta`]. The conductor copies it off the replayed result onto
/// the spawn's unit events (see `conductor::META_MODEL_RESOLVED`), so the recorded events
/// name the concrete model that ran, not only the requested alias on the spawn request.
pub const META_RESOLVED_MODEL: &str = "resolved_model";

/// The [`SpawnResult::meta`] key the headless host records the Claude Code session a spawn ran
/// as under, beside [`META_RESOLVED_MODEL`] on the result it wrote. A spawn answered from that
/// result carries the session back to the conductor, which records it as the spawn's session.
pub const META_SESSION_ID: &str = "session_id";

/// The [`SpawnResult::meta`] key the driver stamps on a spawn it recorded as a LIVENESS
/// fault (spec 10, unit 3): its value is the failure CLASS the taxonomy assigned the hung
/// agent (e.g. `"infra"`). Its presence marks the result as a driver-synthesized liveness
/// outcome - distinct from a worker-reported failure - so the replay driver re-parks it
/// (never charging the unit) and a later real result supersedes it (last-write-wins). No
/// new EVENT TYPE is introduced; the fault rides the existing [`SpawnResult`] on the
/// spawn's id, as the spec requires.
pub const META_LIVENESS_CLASS: &str = "liveness_class";

/// A recorded spawn OUTCOME, keyed by the same deterministic [`spawn_id`] as its
/// request. A successful run carries the agent's `output` and an empty `error`; a
/// failed run carries the failure message in `error`, and the replay driver answers the
/// spawn AS an error - so a pass re-running the conductor over recorded history sees the
/// identical failure it saw live. `meta` carries the driver's optional bookkeeping.
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
    /// recorded failure stays a failure across driver processes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// Optional driver metadata; null when unset and
    /// then omitted from the wire.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub meta: Value,
}

impl SpawnResult {
    /// The one constructor every named result below is a thin variant over: `output` on a
    /// success, `error` on a failure, and no driver metadata.
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

    /// A FAILED result: the spawn errored with `error`. The
    /// replay driver answers a recorded failure with a driver error, never a fake
    /// success.
    pub fn failed(id: impl Into<String>, error: impl Into<String>) -> SpawnResult {
        Self::outcome(id.into(), String::new(), error.into())
    }

    /// A LIVENESS-FAULT result the driver records for a hung spawn (spec 10, unit 3):
    /// the `error` describes the stall and the [`META_LIVENESS_CLASS`] meta key carries the
    /// class the taxonomy assigned it (`class`). It is a driver-synthesized outcome, not a
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

    /// Builder: attach the optional driver metadata.
    pub fn with_meta(mut self, meta: Value) -> Self {
        self.meta = meta;
        self
    }

    /// Whether this result records a FAILURE (a non-empty `error`).
    pub fn is_error(&self) -> bool {
        !self.error.is_empty()
    }

    /// Whether this result is a driver-synthesized LIVENESS fault (spec 10, unit 3): it
    /// carries the [`META_LIVENESS_CLASS`] meta key. The replay driver re-parks such a
    /// result instead of answering the spawn as a charged failure, so a hung agent never
    /// charges the unit a remediation attempt.
    pub fn is_liveness_fault(&self) -> bool {
        self.meta.get(META_LIVENESS_CLASS).is_some()
    }

    /// The string value of the `key` entry of [`meta`](SpawnResult::meta), or empty when none
    /// was recorded (or a non-string value was). The three keys it serves:
    ///
    /// - [`META_LIVENESS_CLASS`]: the liveness class recorded on a fault (empty when it is not
    ///   a liveness fault).
    /// - [`META_RESOLVED_MODEL`]: the RESOLVED model id the headless host read off the session.
    ///   This is the concrete model that actually ran the spawn - distinct from the requested
    ///   alias on the spawn REQUEST - which the conductor copies onto the spawn's unit events
    ///   (spec 05 line 52).
    /// - [`META_SESSION_ID`]: the session the headless host recorded the spawn ran as.
    pub fn meta_str(&self, key: &str) -> String {
        self.meta
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    /// Whether this is an ADJUDICATOR result: the role token of its spawn id (see
    /// [`spawn_role`]) reads as the adjudicator kind through [`review_step_of`]. Only an
    /// adjudicator result carries a gating verdict, so only it disposes a review's findings.
    pub fn is_adjudicator(&self) -> bool {
        matches!(
            review_step_of(spawn_role(&self.id)),
            Some((ReviewKind::Adjudicator, _))
        )
    }

    /// Parse this ADJUDICATOR result's grown verdict line (spec 11) into its
    /// [`Adjudication`] through [`Adjudication::parse`]. Returns `None` when this is not an
    /// adjudicator result, or the output carries no verdict line - the caller then disposes /
    /// attributes nothing. The single disposition-parse authority both the review-quality
    /// metric and the context-graph finding-expiry read.
    pub fn adjudication(&self) -> Option<Adjudication> {
        if !self.is_adjudicator() {
            return None;
        }
        Adjudication::parse(&self.output)
    }
}

impl SpawnEventBody for SpawnResult {
    const EVENT_TYPE: &'static str = TYPE_SPAWN_RESULT;
}

/// The environment variable the build tool reads its cache location from: the ONE home of
/// that name for every rigger surface that sets or reads it. The conductor sets it in a spawn's
/// environment, the shell gate runner in a gate's, the agent driver reads it back from a
/// spawn's, and the managed pre-commit hook reads it to find the build it checks the docs with.
pub const BUILD_CACHE_ENV: &str = "CARGO_TARGET_DIR";

/// One entry of the pending frontier: the identity of a parked spawn that still awaits its
/// result - which spawn, of which unit, at which stage. The dash, `rigger status`, `rigger watch`
/// and the activity view read it; the prompt, persona and placement stay on the recorded
/// [`SpawnRequest`].
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct WaveItem {
    pub id: String,
    pub unit: String,
    pub stage: String,
}

impl From<&SpawnRequest> for WaveItem {
    fn from(req: &SpawnRequest) -> Self {
        WaveItem {
            id: req.id.clone(),
            unit: req.unit.clone(),
            stage: req.stage.clone(),
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
/// [`requests`], [`is_recorded`] and [`recorded_lenient`] all fold over, so a non-spawn
/// event is skipped in exactly one place rather than three times over.
fn spawn_requested_events(events: &[Event]) -> impl Iterator<Item = &Event> {
    events.iter().filter(|e| e.type_ == TYPE_SPAWN_REQUESTED)
}

/// Every [`TYPE_SPAWN_REQUESTED`] event in `events` decoded, in log order, one request per
/// event - the ONE strict per-event decode: [`recorded`] folds it into its map, and a caller
/// that counts parks (a re-parked id counting once per park) reads it directly. A malformed
/// spawn body fails the whole read, never skipped.
pub fn requests(events: &[Event]) -> Result<Vec<SpawnRequest>, serde_json::Error> {
    spawn_requested_events(events)
        .map(SpawnRequest::from_event)
        .collect()
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
    Ok(requests(events)?
        .into_iter()
        .map(|req| (req.id.clone(), req))
        .collect())
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

/// The latest [`TYPE_SPAWN_RESULT`] event of each spawn id in `events`, and the first result
/// body that cannot be read, if any. THE one later-wins pick of a spawn's result: a later result
/// of an id displaces an earlier one, so a corrected re-record supersedes it, and an event of
/// another type is no result. The strict [`latest_result_event`] (and [`result_of`] through it)
/// fails on the unreadable body; the degrade-tolerant [`latest_results_lenient`] skips it, since
/// it carries no id to file it under.
fn latest_results(events: &[Event]) -> (BTreeMap<String, &Event>, Option<serde_json::Error>) {
    let mut latest = BTreeMap::new();
    let mut unreadable = None;
    for event in events.iter().filter(|e| e.type_ == TYPE_SPAWN_RESULT) {
        match SpawnResult::from_event(event) {
            Ok(result) => {
                latest.insert(result.id, event);
            }
            Err(e) => {
                unreadable.get_or_insert(e);
            }
        }
    }
    (latest, unreadable)
}

/// The event of the LATEST recorded result for `id` ([`latest_results`]), for a reader that names
/// the log position it sits at, or `None` if the spawn has no result yet. A malformed result body
/// fails the read, as it fails [`result_of`], which reads this.
pub fn latest_result_event<'a>(
    events: &'a [Event],
    id: &str,
) -> Result<Option<&'a Event>, serde_json::Error> {
    match latest_results(events) {
        (_, Some(unreadable)) => Err(unreadable),
        (latest, None) => Ok(latest.get(id).copied()),
    }
}

/// The LATEST recorded result for `id` ([`latest_results`]), or `None` if the spawn has no
/// result yet (it is still parked at the frontier, awaiting its result). This
/// is how the replay driver decides answer-vs-park: `Some` answers the spawn, `None` parks it.
///
/// Non-result events are ignored and a malformed result body fails the read, just like
/// [`recorded`], so the same run stream feeds this and the ledger/graph projections.
pub fn result_of(events: &[Event], id: &str) -> Result<Option<SpawnResult>, serde_json::Error> {
    latest_result_event(events, id)?
        .map(SpawnResult::from_event)
        .transpose()
}

/// Degrade-tolerant sibling of [`result_of`], as [`recorded_lenient`] is of [`recorded`]: the
/// latest recorded result event of every spawn id in `events` ([`latest_results`]), keyed by
/// the id, a result body that cannot be read SKIPPED rather than failing the read. For a reader
/// one corrupt result must never blind to every other spawn's (the spec critique's union of
/// passes, `review::read_critique`).
pub fn latest_results_lenient(events: &[Event]) -> BTreeMap<String, &Event> {
    latest_results(events).0
}

/// The real result that ENDED spawn `id`: its latest recorded result ([`result_of`]) when that is
/// a real one - the worker's output, or an error a worker or the operator recorded - and `None`
/// while the spawn has not ended: no result yet, or only the liveness fault its host recorded
/// ([`SpawnResult::is_liveness_fault`]). The fault is the host's diagnosis of a silent worker,
/// never its end: the replay driver re-parks such a spawn and a real result recorded later
/// supersedes the fault. THE one spelling of "a spawn has ended" - the replay driver's answer, the
/// liveness rule and the dash's answered set all read it.
pub fn ended_by(events: &[Event], id: &str) -> Result<Option<SpawnResult>, serde_json::Error> {
    Ok(result_of(events, id)?.filter(|res| !res.is_liveness_fault()))
}

/// The run's pending frontier folded from its spawn stream: every parked spawn still awaiting a
/// result, and whether the run has reached a fixpoint.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Step {
    /// The pending frontier, ordered deterministically by [`spawn_id`]. Two ready units with
    /// disjoint blast radii park their spawns together, so fan-out falls out of the run structure.
    pub wave: Vec<WaveItem>,
    /// True when every recorded spawn request has a [`SpawnResult`]: the conductor replayed the
    /// whole log and parked nothing that still awaits a result. A non-empty `wave` always implies
    /// `done == false`, since a freshly parked spawn has no result yet.
    pub done: bool,
}

/// Fold the [`Step`] of the run stream `events`.
///
/// - `wave` is the FULL PENDING FRONTIER: every recorded request with no recorded
///   [`SpawnResult`], never just the spawns one pass newly parked, so a driver that died
///   mid-wave orphans nothing: the relaunched driver sees every unanswered spawn. Ordered by
///   [`spawn_id`] (the [`recorded`] map is keyed by id).
/// - `done` is true when no recorded request still awaits a result. An empty log is vacuously
///   done (nothing to run).
pub fn step_result(events: &[Event]) -> Result<Step, serde_json::Error> {
    let recorded = recorded(events)?;
    // The ids already answered (a recorded result). Folded once, so the wave filter and `done`
    // are O(events) rather than a per-request rescan.
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
    Ok(Step { wave, done })
}

/// A minimal request for the crate's unit tests, defined once with the shared fixtures.
#[cfg(test)]
pub(crate) use crate::test_support::test_request;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::Event;
    use crate::ledger::RequiredItem;

    /// Every scratch walk classifies a per-unit cache by one predicate (spec 113, THE GATE
    /// SCRATCH ROOT HAS ONE LIFECYCLE): the text after a `cargo-target-` or `rigger-gate-`
    /// prefix, empty included, and nothing for any other name.
    #[test]
    fn unit_scratch_slug_names_the_unit_of_a_cache_or_gate_scratch_root_and_nothing_else() {
        assert_eq!(unit_scratch_slug("cargo-target-unit-7"), Some("unit-7"));
        assert_eq!(unit_scratch_slug("rigger-gate-unit-7"), Some("unit-7"));
        assert_eq!(unit_scratch_slug("review-target-unit-7"), Some("unit-7"));
        assert_eq!(unit_scratch_slug("review-target-"), Some(""));
        assert_eq!(unit_scratch_slug("review-target"), None);
        assert_eq!(unit_scratch_slug("cargo-target-"), Some(""));
        assert_eq!(unit_scratch_slug("rigger-gate-"), Some(""));
        assert_eq!(unit_scratch_slug("cargo-target"), None);
        assert_eq!(unit_scratch_slug("rigger-gate"), None);
        assert_eq!(unit_scratch_slug("rigger-wt-unit-7"), None);
        assert_eq!(unit_scratch_slug("mutation-tool-unit-7"), None);
        assert_eq!(unit_scratch_slug("x-rigger-gate-unit-7"), None);
        assert_eq!(unit_scratch_slug(""), None);
    }

    /// A reject's verdict line names the items it requires fixed - each one's finding, the
    /// file it is in, whether it is a correctness defect, and the shape of a defect that
    /// recurs across sites - read by the one verdict-line parse beside the rest of the
    /// disposition; a line that names none, or names them in another shape, requires nothing.
    #[test]
    fn required_items_parse_from_the_verdict_line() {
        let required = |output: &str| {
            Adjudication::parse(output)
                .map(|a| a.required)
                .unwrap_or_default()
        };
        let line = r#"{"verdict":"reject","required":[{"finding":"scope the log read","path":"src/a.rs","correctness":true,"pattern":"unscoped log reads"},{"finding":"reword the doc","path":"docs/b.md"}]}"#;
        assert_eq!(
            required(&format!("the reasoning\n{line}")),
            vec![
                RequiredItem {
                    finding: "scope the log read".into(),
                    path: "src/a.rs".into(),
                    correctness: true,
                    pattern: "unscoped log reads".into(),
                },
                RequiredItem {
                    finding: "reword the doc".into(),
                    path: "docs/b.md".into(),
                    correctness: false,
                    pattern: String::new(),
                },
            ]
        );
        assert_eq!(
            required(&format!("{line}\n{{\"verdict\":\"approve\"}}")),
            Vec::new(),
            "the last verdict line is the one read"
        );
        for none in [
            r#"{"verdict":"reject"}"#,
            r#"{"verdict":"reject","required":"fix it"}"#,
            r#"{"verdict":"reject","required":["fix it"]}"#,
            r#"{"required":[{"finding":"not a verdict line"}]}"#,
            "no json here",
        ] {
            assert_eq!(required(none), Vec::new(), "{none}");
        }
    }

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

    /// The plan-critique spec-defect cause (spec 112, criterion 5) is spelled once, and a verdict
    /// is a spec ambiguity exactly when its parsed cause is that spelling: never for another
    /// cause, a cause in another case, or a verdict naming none.
    #[test]
    fn a_verdict_is_a_spec_ambiguity_exactly_when_its_cause_is_spec_ambiguity() {
        let is_spec_ambiguity = |line: &str| Adjudication::parse(line).unwrap().is_spec_ambiguity();
        assert_eq!(
            (
                CAUSE_SPEC_AMBIGUITY,
                is_spec_ambiguity(r#"{"verdict":"reject","cause":"spec-ambiguity"}"#),
                is_spec_ambiguity(r#"{"verdict":"reject","cause":"decomposition-conflict"}"#),
                is_spec_ambiguity(r#"{"verdict":"reject","cause":"infra-fault"}"#),
                is_spec_ambiguity(r#"{"verdict":"reject","cause":"Spec-Ambiguity"}"#),
                is_spec_ambiguity(r#"{"verdict":"reject","upheld":["a1"]}"#),
            ),
            ("spec-ambiguity", true, false, false, false, false)
        );
    }

    /// STEP ROLE TOKENS (spec 120): a step spells its kind's token, and from the second step of
    /// that kind `@<k>` after it, while the adjudicator, one step at most, never spells `@k`.
    #[test]
    fn a_review_step_role_spells_its_kinds_token_and_an_ordinal_from_the_second_step() {
        for (kind, agent, k, token) in [
            (ReviewKind::Lens, "c", 1, "lens:c"),
            (ReviewKind::Lens, "c", 2, "lens:c@2"),
            (ReviewKind::Lens, "c", 10, "lens:c@10"),
            (ReviewKind::Adversary, "x", 1, "adversary"),
            (ReviewKind::Adversary, "x", 2, "adversary@2"),
            (ReviewKind::Adversary, "x", 3, "adversary@3"),
            (ReviewKind::Adjudicator, "z", 1, "adjudicator"),
            (ReviewKind::Adjudicator, "z", 2, "adjudicator"),
        ] {
            assert_eq!(review_step_role(kind, agent, k), token, "{kind:?} {k}");
        }
    }

    /// The one parser of a review token: a bare token is step 1, the suffix is `@` and an ASCII
    /// decimal of at least 2 that fits a `u32`, with no leading zero and no sign, and `@k` exists
    /// for the lens and adversary kinds only. `lens_agent_of` reads the same spellings: a lens
    /// token names its agent whichever step it sits in, and anything else names none.
    #[test]
    fn the_token_readers_read_each_token_and_refuse_every_other_spelling() {
        use ReviewKind::{Adjudicator, Adversary, Lens};
        for (role, read, agent) in [
            ("lens:a", Some((Lens, 1)), Some("a")),
            ("adversary", Some((Adversary, 1)), None),
            ("adjudicator", Some((Adjudicator, 1)), None),
            ("lens:", Some((Lens, 1)), Some("")),
            ("lens:c@2", Some((Lens, 2)), Some("c")),
            (
                "lens:architecture-reviewer@4294967295",
                Some((Lens, u32::MAX)),
                Some("architecture-reviewer"),
            ),
            ("adversary@2", Some((Adversary, 2)), None),
            ("adversary@10", Some((Adversary, 10)), None),
            ("adversary@4294967295", Some((Adversary, u32::MAX)), None),
            ("adversary@4294967296", None, None),
            ("lens:a@4294967296", None, None),
            ("adversary@1", None, None),
            ("adversary@0", None, None),
            ("adversary@01", None, None),
            ("adversary@02", None, None),
            ("adversary@+2", None, None),
            ("adversary@-2", None, None),
            ("adversary@", None, None),
            ("adversary@x", None, None),
            ("adversary@2x", None, None),
            ("lens:a@1", None, None),
            ("lens:a@01", None, None),
            ("lens:a@02", None, None),
            ("lens:a@x", None, None),
            ("lens:a@2@3", None, None),
            ("lens:a@@2", None, None),
            ("adjudicator@2", None, None),
            ("adjudicator@1", None, None),
            ("implementer", None, None),
            ("implementer@2", None, None),
            ("sdet-author", None, None),
            ("lensing", None, None),
            ("@2", None, None),
            ("", None, None),
        ] {
            assert_eq!(review_step_of(role), read, "{role:?}");
            assert_eq!(lens_agent_of(role), agent, "{role:?}");
        }
    }

    /// A kind's tier is the label its spawns fold and group under, the seeds' own spellings.
    #[test]
    fn a_review_kind_reads_as_its_tier_label() {
        for (tier, label) in [
            (ReviewKind::Lens.tier(), "lens"),
            (ReviewKind::Adversary.tier(), "adversary"),
            (ReviewKind::Adjudicator.tier(), "adjudicator"),
            (TIER_LENS, "lens"),
            (TIER_ADVERSARY, "adversary"),
        ] {
            assert_eq!(tier, label);
        }
    }

    /// `review_step_of` is the inverse of `review_step_role` over `spawn_role`'s output, a
    /// `~retry{n}` respawn suffix trimmed: a token survives the spawn id it rides.
    #[test]
    fn a_minted_review_token_reads_back_through_its_spawn_id() {
        for (kind, k) in [
            (ReviewKind::Lens, 1),
            (ReviewKind::Lens, 2),
            (ReviewKind::Lens, u32::MAX),
            (ReviewKind::Adversary, 1),
            (ReviewKind::Adversary, 2),
            (ReviewKind::Adversary, u32::MAX),
            (ReviewKind::Adjudicator, 1),
        ] {
            let role = review_step_role(kind, "agent", k);
            for id in [spawn_id("u", &role, 3), spawn_retry_id("u", &role, 3, 2)] {
                assert_eq!(spawn_role(&id), role, "{id}");
                assert_eq!(review_step_of(spawn_role(&id)), Some((kind, k)), "{id}");
            }
        }
    }

    /// Only the adjudicator's token carries a gating verdict: a retry of it still does, and no
    /// other token reads as one, an `adjudicator@k` no minter spells included.
    #[test]
    fn is_adjudicator_reads_the_token_of_the_spawn_id() {
        for (id, adjudicator) in [
            ("u/adjudicator#0", true),
            ("u/adjudicator#0~retry2", true),
            ("u/adjudicator@2#0", false),
            ("u/adversary#0", false),
            ("u/adversary@2#0", false),
            ("u/lens:adjudicator#0", false),
            ("u/lens:a@2#0", false),
            ("u/implementer#0", false),
        ] {
            assert_eq!(
                SpawnResult::ok(id, "").is_adjudicator(),
                adjudicator,
                "{id}"
            );
        }
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
        // Gap 18 replay-safety: two driver processes replaying the same history must
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
        // driver parks and answers each independently.
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
            .with_meta(serde_json::json!({"by": "driver"}));
        assert!(res.is_error());
        assert_eq!(res.error, "agent crashed: non-zero exit");
        assert_eq!(res.meta, serde_json::json!({"by": "driver"}));

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
        // Present: persisted and recovered so the host reads it off the parked event.
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
        // spec 05 line 52: the driver reports the resolved model in the result's `meta`
        // (`{"resolved_model": ..}`); `meta_str(META_RESOLVED_MODEL)` reads exactly that key so the
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
                .with_meta(serde_json::json!({ "by": "driver" }))
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

    /// THE later-wins pick of a spawn's result, read both ways over one stream: the latest result
    /// of each spawn id wins and an event of another type is no result; the strict [`result_of`]
    /// fails on the first unreadable result body, and [`latest_results_lenient`] skips it.
    #[test]
    fn a_spawns_result_is_its_latest_read_strictly_or_past_an_unreadable_body() {
        let result = |id: &str, output: &str| SpawnResult::ok(id, output).to_event().unwrap();
        let no_id = Event::new(TYPE_SPAWN_RESULT, br#"{"output":"no id"}"#.to_vec());
        let not_json = Event::new(TYPE_SPAWN_RESULT, b"not json".to_vec());
        let unreadable = |event: &Event| SpawnResult::from_event(event).unwrap_err().to_string();
        // A request whose body reads as a result of `a`: only its type keeps it out.
        let request = Event::new(TYPE_SPAWN_REQUESTED, result("a", "a request").data);
        type Row<'a> = (
            &'a str,
            Vec<Event>,
            Vec<(&'a str, &'a str)>,
            Result<Option<&'a str>, String>,
        );
        let rows: [Row; 5] = [
            ("no event", vec![], vec![], Ok(None)),
            (
                "a later result of an id displaces an earlier one",
                vec![
                    result("a", "first"),
                    result("b", "other"),
                    result("a", "second"),
                ],
                vec![("a", "second"), ("b", "other")],
                Ok(Some("second")),
            ),
            (
                "an event of another type is no result",
                vec![request, result("b", "other")],
                vec![("b", "other")],
                Ok(None),
            ),
            (
                "an unreadable body between two results of the id",
                vec![result("a", "first"), no_id.clone(), result("a", "second")],
                vec![("a", "second")],
                Err(unreadable(&no_id)),
            ),
            (
                "two unreadable bodies",
                vec![no_id.clone(), not_json],
                vec![],
                Err(unreadable(&no_id)),
            ),
        ];
        for (why, events, lenient, strict) in rows {
            let read: Vec<(String, String)> = latest_results_lenient(&events)
                .into_iter()
                .map(|(id, event)| (id, SpawnResult::from_event(event).unwrap().output))
                .collect();
            let want: Vec<(String, String)> = lenient
                .iter()
                .map(|(id, output)| (id.to_string(), output.to_string()))
                .collect();
            assert_eq!(read, want, "{why}: every id's latest result");
            assert_eq!(
                result_of(&events, "a")
                    .map(|res| res.map(|res| res.output))
                    .map_err(|e| e.to_string()),
                strict.map(|output| output.map(str::to_string)),
                "{why}: the strict read of a"
            );
        }
    }

    /// THE spawn-has-ended rule: a spawn has ended once its latest result is a real one - a
    /// worker's output or a recorded error - and never on the step's liveness fault, which a
    /// real result recorded later supersedes.
    #[test]
    fn a_spawn_ends_on_its_latest_real_result_never_on_a_liveness_fault() {
        let id = "u/implementer#0";
        let ok = || SpawnResult::ok(id, "done").to_event().unwrap();
        let failed = || SpawnResult::failed(id, "worker gone").to_event().unwrap();
        let fault = || {
            SpawnResult::liveness_fault(id, "hung", "infra")
                .to_event()
                .unwrap()
        };
        let ended = |events: &[Event]| ended_by(events, id).unwrap().map(|res| res.error);
        assert_eq!(ended(&[]), None, "no result: still in flight");
        assert_eq!(ended(&[fault()]), None, "a liveness fault is no end");
        assert_eq!(
            ended(&[ok()]),
            Some(String::new()),
            "a worker's output ends it"
        );
        assert_eq!(
            ended(&[fault(), failed()]),
            Some("worker gone".to_string()),
            "a real error recorded after the fault ends it"
        );
        assert_eq!(
            ended(&[ok(), fault()]),
            None,
            "the latest result decides, as everywhere else"
        );
        assert_eq!(
            ended_by(&[ok()], "u/implementer#1").unwrap(),
            None,
            "another spawn's result ends nothing"
        );
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
    fn requests_decodes_one_request_per_spawn_event_in_log_order_and_recorded_keeps_the_last_per_id(
    ) {
        // The ONE strict per-event decode: every `SpawnRequested` event in log order, a re-parked
        // id once per park, a foreign event or a result skipped. `recorded` folds it into its map,
        // where a re-parked id collapses to the last-written request.
        let first = test_request("u", "implement", ROLE_IMPLEMENTER, 0, "first");
        let other = test_request("v", "implement", ROLE_IMPLEMENTER, 0, "other");
        let reparked = test_request("u", "implement", ROLE_IMPLEMENTER, 0, "reparked");
        let events = vec![
            first.to_event().unwrap(),
            Event::new("UnitStarted", br#"{"id":"u"}"#.to_vec()),
            other.to_event().unwrap(),
            SpawnResult::ok(&first.id, "done").to_event().unwrap(),
            reparked.to_event().unwrap(),
        ];
        assert_eq!(
            requests(&events).unwrap(),
            vec![first.clone(), other.clone(), reparked.clone()]
        );
        assert_eq!(
            recorded(&events).unwrap(),
            BTreeMap::from([(reparked.id.clone(), reparked), (other.id.clone(), other)])
        );
        assert_eq!(requests(&events[1..2]).unwrap(), vec![]);
    }

    #[test]
    fn an_unreadable_spawn_request_fails_requests_and_recorded_alike() {
        let unreadable = Event::new(
            TYPE_SPAWN_REQUESTED,
            br#"{"id":"u/implementer#1"}"#.to_vec(),
        );
        let events = vec![
            test_request("u", "implement", ROLE_IMPLEMENTER, 0, "p")
                .to_event()
                .unwrap(),
            unreadable,
        ];
        assert_eq!(
            requests(&events).unwrap_err().to_string(),
            "missing field `unit` at line 1 column 24"
        );
        assert_eq!(
            recorded(&events).unwrap_err().to_string(),
            "missing field `unit` at line 1 column 24"
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
    fn a_spawn_requests_title_rides_the_wire_and_is_omitted_when_empty() {
        // The live work-line (spec 19a, c4): a spawn carries its unit's criterion as a
        // `title` so a driver can narrate the actual WORK, not just `<unit>:<stage>`.
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
    fn a_spawn_requests_reviews_roster_rides_the_wire_and_is_omitted_when_empty() {
        // REVIEW TIERS NAME THEIR TARGETS (spec 67, criterion 4): a review-tier spawn
        // (adversary/adjudicator) carries the unit's routed lens roster it judges, so the
        // driver can name it in the action phrase. An empty roster
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
    fn the_frontier_keeps_every_unanswered_spawn_so_a_dead_driver_orphans_nothing() {
        // A driver that died after parking a spawn must not orphan it: the frontier folded
        // afterwards still holds every spawn awaiting a result, so a relaunched driver resumes
        // the in-flight wave; the answered spawn does not reappear.
        let a = test_request("a", "implement", ROLE_IMPLEMENTER, 0, "a");
        let b = test_request("b", "implement", ROLE_IMPLEMENTER, 0, "b");

        // `a` was answered; `b` was parked when its driver died.
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
            "the frontier holds the unanswered spawn and only it"
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
    fn step_serializes_the_frontier_identity_and_a_done_bool() {
        // The dash ships this Step in its state JSON: each wave item names its spawn, unit and
        // stage, and the prompt, persona and placement stay on the recorded request.
        let events = vec![test_request("u", "implement", ROLE_IMPLEMENTER, 0, "do it")
            .to_event()
            .unwrap()];
        assert_eq!(
            serde_json::to_string(&step_result(&events).unwrap()).unwrap(),
            r#"{"wave":[{"id":"u/implementer#0","unit":"u","stage":"implement"}],"done":false}"#
        );
    }
}
