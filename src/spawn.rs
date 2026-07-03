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

use crate::conductor::STREAM;
use crate::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision, Position};

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
}

impl SpawnRequest {
    /// Build a request, deriving its deterministic id from `unit` + `role` +
    /// `attempt` so the id can never drift out of sync with the labels it is derived
    /// from. Optional fields (persona, model, tools, dir, blast-radius) are filled in
    /// with the builder setters.
    pub fn new(unit: &str, stage: &str, role: &str, attempt: u32, prompt: &str) -> SpawnRequest {
        SpawnRequest {
            id: spawn_id(unit, role, attempt),
            unit: unit.to_string(),
            stage: stage.to_string(),
            prompt: prompt.to_string(),
            system_prompt: String::new(),
            model: String::new(),
            tools: Vec::new(),
            dir: String::new(),
            blast_radius: Vec::new(),
        }
    }

    /// Builder: set the agent's persona (system prompt).
    pub fn with_system_prompt(mut self, persona: impl Into<String>) -> Self {
        self.system_prompt = persona.into();
        self
    }

    /// Builder: set the model alias.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Builder: set the granted tools.
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    /// Builder: set the working dir.
    pub fn with_dir(mut self, dir: impl Into<String>) -> Self {
        self.dir = dir.into();
        self
    }

    /// Builder: set the blast-radius (grounded seed files).
    pub fn with_blast_radius(mut self, blast_radius: Vec<String>) -> Self {
        self.blast_radius = blast_radius;
        self
    }

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

/// Persist a parked spawn request to the run's event log as a
/// [`TYPE_SPAWN_REQUESTED`] event, returning its global position.
///
/// This is exactly what a step does when it reaches an UNRECORDED spawn at the
/// frontier: the request becomes a durable fact, so the next step process (and the
/// thin driver draining the wave) sees the identical call, and the budget breaker
/// counts spawns from the log rather than an in-memory counter. A serialization
/// failure is surfaced as a backend error rather than panicking.
pub fn park(store: &dyn EventStore, req: &SpawnRequest) -> Result<Position, Error> {
    park_in_run(store, req, "")
}

/// Park `req` as a [`TYPE_SPAWN_REQUESTED`] event stamped with the run it belongs to,
/// so the parked spawn is attributable to its run (spec 06, unit 1): the conductor
/// threads the current run id onto every spawn and the replay driver parks through
/// here, so a `SpawnRequested` carries the same `run_id` metadata as the unit/gate
/// events the conductor emits for that run. An empty `run_id` stamps no metadata (a
/// caller outside a run - e.g. the pure-fold tests), so [`park`] is exactly this with
/// no run. This is the single park authority; [`park`] delegates to it.
pub fn park_in_run(
    store: &dyn EventStore,
    req: &SpawnRequest,
    run_id: &str,
) -> Result<Position, Error> {
    let mut ev = req
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize spawn request {}: {e}", req.id)))?;
    if !run_id.is_empty() {
        ev = ev.with_meta(crate::run::META_RUN_ID, run_id);
    }
    store.append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))
}

/// Fold the [`TYPE_SPAWN_REQUESTED`] events in `events` into the spawn requests
/// already parked, keyed by their deterministic id.
///
/// The replay driver uses this to tell an already-recorded spawn (answer it from the
/// log) from an unrecorded one (park it); the budget breaker counts the entries.
/// Non-spawn events are ignored, so the same run stream feeds this and the
/// ledger/graph projections. A re-parked id (an idempotency violation the replay
/// driver is responsible for preventing) collapses to the last-written request.
pub fn recorded(events: &[Event]) -> Result<BTreeMap<String, SpawnRequest>, serde_json::Error> {
    let mut out = BTreeMap::new();
    for e in events {
        if e.type_ == TYPE_SPAWN_REQUESTED {
            let req = SpawnRequest::from_event(e)?;
            out.insert(req.id.clone(), req);
        }
    }
    Ok(out)
}

/// Whether a spawn with `id` has already been parked in `events` - a cheap
/// membership check over [`recorded`] for the replay driver's park-or-replay
/// decision. A malformed spawn event never matches (it cannot carry a valid id).
pub fn is_recorded(events: &[Event], id: &str) -> bool {
    events.iter().any(|e| {
        e.type_ == TYPE_SPAWN_REQUESTED && SpawnRequest::from_event(e).is_ok_and(|r| r.id == id)
    })
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

    /// Builder: attach the optional courier metadata (`rigger result --meta <json>`).
    pub fn with_meta(mut self, meta: Value) -> Self {
        self.meta = meta;
        self
    }

    /// Whether this result records a FAILURE (a non-empty `error`).
    pub fn is_error(&self) -> bool {
        !self.error.is_empty()
    }

    /// The RESOLVED model id the worker reported through `--meta` (the
    /// [`META_RESOLVED_MODEL`] key of [`meta`](SpawnResult::meta)), or empty when the
    /// worker reported none (or reported a non-string value). This is the concrete model
    /// that actually ran the spawn - distinct from the requested alias on the spawn
    /// REQUEST - which the conductor copies onto the spawn's unit events (spec 05 line 52).
    pub fn resolved_model(&self) -> String {
        self.meta
            .get(META_RESOLVED_MODEL)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
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

/// Persist a recorded spawn result to the run's event log as a [`TYPE_SPAWN_RESULT`]
/// event, returning its global position. This is exactly what `rigger result <id>`
/// does once a courier has run the parked agent: the outcome becomes a durable fact,
/// so the next step process replays it instead of re-running the agent.
pub fn record_result(store: &dyn EventStore, res: &SpawnResult) -> Result<Position, Error> {
    let ev = res
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize spawn result {}: {e}", res.id)))?;
    store.append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))
}

/// Record `res` to the run's event log ONLY when the spawn has no result yet, as a
/// single atomic compare-and-append that never clobbers a result already recorded - the
/// write half of `rigger result --if-absent`. Returns `Some(position)` when it recorded,
/// `None` when a result already existed (the idempotent no-op).
///
/// The thin driver's death courier calls this to record a died-worker failure IFF the
/// worker did not already self-report. It supersedes the two-process `rigger reported
/// <id> || rigger result <id> --error` guard, which reads in one process and writes in
/// another and so leaves a TOCTOU window: a self-report (or a reviewer's already-emitted
/// approve) landing between the read and the write is clobbered by the courier's
/// `--error` - since [`record_result`]/[`result_of`] are last-write-wins - force-failing
/// an approved unit on the next replay. Collapsing the check and the write into one
/// atomic operation closes that window.
///
/// Atomicity rests on the store's optimistic concurrency (the port's only cross-backend
/// primitive): read the stream, and if no [`TYPE_SPAWN_RESULT`] for `res.id` is present,
/// append under an [`ExpectedRevision`] pinned to the revision just read. A concurrent
/// append that landed after the read (the racing self-report, or any other writer) makes
/// that expectation CONFLICT; we re-read and re-decide, so the write lands at most once
/// and a self-report that won the race is honored (the re-check now sees it and returns
/// `None`). Only a genuine [`Error::Conflict`] retries; any other backend error surfaces.
pub fn record_result_if_absent(
    store: &dyn EventStore,
    res: &SpawnResult,
) -> Result<Option<Position>, Error> {
    let ev = res
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize spawn result {}: {e}", res.id)))?;
    loop {
        let events = store.read_stream(STREAM, 0, Direction::Forward)?;
        if result_of(&events, &res.id)
            .map_err(|e| Error::Backend(format!("decode results for {}: {e}", res.id)))?
            .is_some()
        {
            // A result already exists - leave it untouched (the no-op the courier wants).
            return Ok(None);
        }
        // Pin the append to the exact revision we just read: any event appended since
        // (Forward reads ascending, so `.last()` is the current head) fails the check.
        let expected = match events.last() {
            Some(e) => ExpectedRevision::Exact(e.revision),
            None => ExpectedRevision::NoStream,
        };
        match store.append(STREAM, expected, std::slice::from_ref(&ev)) {
            Ok(pos) => return Ok(Some(pos)),
            // The stream moved under us; re-read and re-decide. If the racing writer
            // recorded THIS id, the re-check returns `None` and nothing is clobbered.
            Err(Error::Conflict { .. }) => continue,
            Err(e) => return Err(e),
        }
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
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blast_radius: Vec<String>,
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
    Ok(Step { wave, done })
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
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, Filter, Revision, Subscription};

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
    fn new_derives_the_id_from_the_labels_and_carries_the_prompt() {
        let req = SpawnRequest::new("spawn-req", "implement", ROLE_IMPLEMENTER, 1, "do it");
        assert_eq!(req.id, "spawn-req/implementer#1");
        assert_eq!(req.unit, "spawn-req");
        assert_eq!(req.stage, "implement");
        assert_eq!(req.prompt, "do it");
    }

    #[test]
    fn a_request_carries_every_field_the_thin_driver_needs() {
        let req = SpawnRequest::new("u", "implement", ROLE_IMPLEMENTER, 0, "prompt")
            .with_system_prompt("You are the rust engineer.")
            .with_model("sonnet")
            .with_tools(vec!["Read".into(), "Edit".into()])
            .with_dir("/wt")
            .with_blast_radius(vec!["a.rs".into()]);

        assert_eq!(req.system_prompt, "You are the rust engineer.");
        assert_eq!(req.model, "sonnet");
        assert_eq!(req.tools, ["Read", "Edit"]);
        assert_eq!(req.dir, "/wt");
        assert_eq!(req.blast_radius, ["a.rs"]);
    }

    #[test]
    fn a_request_round_trips_through_its_event() {
        let req = SpawnRequest::new("u", "implement", ROLE_IMPLEMENTER, 0, "prompt")
            .with_system_prompt("persona")
            .with_model("sonnet")
            .with_tools(vec!["Read".into()])
            .with_dir("/wt")
            .with_blast_radius(vec!["a.rs".into()]);

        let ev = req.to_event().unwrap();
        assert_eq!(ev.type_, TYPE_SPAWN_REQUESTED);
        assert_eq!(SpawnRequest::from_event(&ev).unwrap(), req);
    }

    #[test]
    fn empty_optional_fields_are_omitted_from_the_wire() {
        // A minimal request serializes to only the always-present fields, so a
        // persisted spawn event and a printed wave stay compact.
        let req = SpawnRequest::new("u", "implement", ROLE_IMPLEMENTER, 0, "prompt");
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
    fn parking_persists_the_request_and_it_folds_back_from_the_log() {
        let store = Store::open(":memory:").unwrap();
        let req = SpawnRequest::new("u", "implement", ROLE_IMPLEMENTER, 0, "do it")
            .with_model("sonnet")
            .with_blast_radius(vec!["a.rs".into()]);

        park(&store, &req).unwrap();

        // The parked request is a durable fact on the run stream and reads back
        // identically - the persistence the replay driver and budget breaker rely on.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let recorded = recorded(&events).unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[&req.id], req);
        assert!(is_recorded(&events, &req.id));
        assert!(!is_recorded(&events, "u/implementer#1"));
    }

    #[test]
    fn recorded_keys_two_disjoint_spawns_of_one_wave_by_id() {
        // Two ready units park their spawns in the same wave (the fan-out shape); the
        // fold keys them by their distinct ids so the driver can drain both.
        let store = Store::open(":memory:").unwrap();
        let a = SpawnRequest::new("a", "implement", ROLE_IMPLEMENTER, 0, "a");
        let b = SpawnRequest::new("b", "implement", ROLE_IMPLEMENTER, 0, "b");
        park(&store, &a).unwrap();
        park(&store, &b).unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let recorded = recorded(&events).unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[&a.id].unit, "a");
        assert_eq!(recorded[&b.id].unit, "b");
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
    fn resolved_model_reads_the_meta_key_the_worker_reports() {
        // spec 05 line 52: the worker reports the resolved model via `rigger result --meta
        // '{"resolved_model": ..}'`; `resolved_model()` reads exactly that key so the
        // conductor can copy it onto the spawn's unit events.
        let with = SpawnResult::ok("u/implementer#0", "done")
            .with_meta(serde_json::json!({ "resolved_model": "claude-opus-4-8-20260101" }));
        assert_eq!(with.resolved_model(), "claude-opus-4-8-20260101");

        // No meta, wrong key, or a non-string value each read as empty (then omitted).
        assert_eq!(
            SpawnResult::ok("u/implementer#0", "done").resolved_model(),
            ""
        );
        assert_eq!(
            SpawnResult::ok("u/implementer#0", "done")
                .with_meta(serde_json::json!({ "by": "courier" }))
                .resolved_model(),
            ""
        );
        assert_eq!(
            SpawnResult::ok("u/implementer#0", "done")
                .with_meta(serde_json::json!({ "resolved_model": 7 }))
                .resolved_model(),
            ""
        );
    }

    #[test]
    fn recording_a_result_persists_it_and_result_of_reads_it_back() {
        let store = Store::open(":memory:").unwrap();
        // No result yet -> the spawn is still parked at the frontier.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(result_of(&events, "u/implementer#0").unwrap().is_none());

        record_result(&store, &SpawnResult::ok("u/implementer#0", "done")).unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert_eq!(got.output, "done");
        // A different id has no result of its own.
        assert!(result_of(&events, "u/implementer#1").unwrap().is_none());
    }

    #[test]
    fn result_of_returns_the_latest_recorded_result_for_an_id() {
        // A corrected re-record supersedes an earlier result (last write wins).
        let store = Store::open(":memory:").unwrap();
        record_result(&store, &SpawnResult::failed("u/implementer#0", "flaked")).unwrap();
        record_result(&store, &SpawnResult::ok("u/implementer#0", "recovered")).unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert!(
            !got.is_error(),
            "the later success supersedes the earlier failure"
        );
        assert_eq!(got.output, "recovered");
    }

    #[test]
    fn record_result_if_absent_records_only_when_no_result_exists() {
        // The write half of `rigger result --if-absent`: with no result yet it records,
        // returning the new position, and `result_of` reads it back.
        let store = Store::open(":memory:").unwrap();
        let pos =
            record_result_if_absent(&store, &SpawnResult::ok("u/implementer#0", "done")).unwrap();
        assert!(pos.is_some(), "an absent result must be recorded");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert_eq!(got.output, "done");
    }

    #[test]
    fn record_result_if_absent_is_a_noop_that_never_clobbers_an_existing_result() {
        // The anti-clobber invariant the death courier relies on: once a worker has
        // self-reported, a later `--if-absent` (the courier's died-worker `--error`)
        // records NOTHING and leaves the self-report standing - the same guarantee the
        // two-process `rigger reported <id> || rigger result <id> --error` guard gave,
        // now in ONE atomic step so no self-report can land in the check-then-record gap.
        let store = Store::open(":memory:").unwrap();
        record_result(&store, &SpawnResult::ok("u/implementer#0", "self-reported")).unwrap();

        let skipped = record_result_if_absent(
            &store,
            &SpawnResult::failed("u/implementer#0", "died without reporting"),
        )
        .unwrap();
        assert!(
            skipped.is_none(),
            "an already-recorded result must not be re-recorded (return None)"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let results = events
            .iter()
            .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
            .count();
        assert_eq!(
            results, 1,
            "the `--if-absent` no-op must append no second result event"
        );
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert!(
            !got.is_error(),
            "the self-reported success must stand un-clobbered"
        );
        assert_eq!(got.output, "self-reported");
    }

    /// A store wrapper that simulates a CONCURRENT writer committing in the window
    /// between `record_result_if_absent`'s `read_stream` and its compare-and-append:
    /// on the FIRST append it slips `racing` onto the stream (under `Any`, so it always
    /// lands and advances the head), which makes the caller's revision-pinned append
    /// CONFLICT. This drives the `Err(Error::Conflict) => continue` retry arm
    /// DETERMINISTICALLY every run - the arm that IS the "records atomically" guarantee,
    /// which a purely sequential test never reaches. Every other method delegates
    /// straight through to the real store.
    struct RaceOnFirstAppend {
        inner: Store,
        racing: std::sync::Mutex<Option<Event>>,
    }

    impl RaceOnFirstAppend {
        fn new(inner: Store, racing: Event) -> Self {
            Self {
                inner,
                racing: std::sync::Mutex::new(Some(racing)),
            }
        }
    }

    impl EventStore for RaceOnFirstAppend {
        fn append(
            &self,
            stream: &str,
            expected: ExpectedRevision,
            events: &[Event],
        ) -> Result<Position, Error> {
            // The concurrent writer: land it once, just before the caller's first
            // append, so the stream head moves under the caller's pinned expectation
            // and the real store returns a genuine Conflict.
            if let Some(ev) = self.racing.lock().unwrap().take() {
                self.inner
                    .append(stream, ExpectedRevision::Any, std::slice::from_ref(&ev))?;
            }
            self.inner.append(stream, expected, events)
        }

        fn read_stream(
            &self,
            stream: &str,
            from: Revision,
            dir: Direction,
        ) -> Result<Vec<Event>, Error> {
            self.inner.read_stream(stream, from, dir)
        }

        fn read_all(
            &self,
            from: Position,
            dir: Direction,
            filter: &Filter,
        ) -> Result<Vec<Event>, Error> {
            self.inner.read_all(from, dir, filter)
        }

        fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
            self.inner.subscribe_all(from, filter)
        }

        fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
            self.inner.subscribe_stream(stream, from)
        }
    }

    #[test]
    fn record_result_if_absent_retries_when_a_racing_append_conflicts() {
        // A DIFFERENT writer commits between our read and our compare-and-append, so the
        // revision-pinned append CONFLICTS. The retry loop must re-read, re-decide, and -
        // since THIS id still has no result - land it exactly once. Recording the absent
        // result over a moving stream is the whole point of the loop; if the
        // `Err(Conflict) => continue` arm is dropped (e.g. replaced by a panic or an
        // early return) this test fails.
        let inner = Store::open(":memory:").unwrap();
        // The racing writer records some OTHER unit's result (an unrelated concurrent
        // courier), so after the conflict our id is still absent and must be recorded.
        let racing = SpawnResult::ok("other/implementer#0", "unrelated")
            .to_event()
            .unwrap();
        let store = RaceOnFirstAppend::new(inner, racing);

        let pos =
            record_result_if_absent(&store, &SpawnResult::ok("u/implementer#0", "done")).unwrap();
        assert!(
            pos.is_some(),
            "the racing append forced a conflict; the retry must still record the absent result"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        // Exactly one result for OUR id - recorded once, not duplicated by the retry.
        let ours = events
            .iter()
            .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
            .filter_map(|e| SpawnResult::from_event(e).ok())
            .filter(|r| r.id == "u/implementer#0")
            .count();
        assert_eq!(ours, 1, "the retry must record our result exactly once");
        assert_eq!(
            result_of(&events, "u/implementer#0")
                .unwrap()
                .unwrap()
                .output,
            "done"
        );
        // The concurrent writer's unrelated record survives alongside it (nothing lost).
        assert!(
            result_of(&events, "other/implementer#0").unwrap().is_some(),
            "the concurrent writer's record must survive the retry"
        );
    }

    #[test]
    fn record_result_if_absent_honors_a_self_report_that_won_the_race() {
        // The TOCTOU window the atomic CAS closes: the worker's own self-report lands in
        // the gap between the courier's read (which saw nothing) and its append. The
        // pinned append CONFLICTS; on retry the re-check now SEES the self-report and
        // returns None, so the courier's died-worker `--error` never clobbers the
        // success. Dropping either the retry arm or the in-loop re-check fails this.
        let inner = Store::open(":memory:").unwrap();
        // The racing writer is the worker itself, self-reporting SUCCESS for OUR id.
        let racing = SpawnResult::ok("u/implementer#0", "self-reported")
            .to_event()
            .unwrap();
        let store = RaceOnFirstAppend::new(inner, racing);

        // The death courier, believing the worker died, fires `--if-absent --error`.
        let skipped =
            record_result_if_absent(&store, &SpawnResult::failed("u/implementer#0", "died"))
                .unwrap();
        assert!(
            skipped.is_none(),
            "the self-report won the race; the re-check on retry must make this a no-op"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let results = events
            .iter()
            .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
            .count();
        assert_eq!(
            results, 1,
            "the losing courier must append no second result (no clobber, no duplicate)"
        );
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert!(
            !got.is_error(),
            "the self-reported success must stand, not be force-failed by the courier"
        );
        assert_eq!(got.output, "self-reported");
    }

    #[test]
    fn record_result_if_absent_is_atomic_across_two_connections() {
        // The criterion on the REAL topology: the death courier runs in a SEPARATE
        // PROCESS from the worker, so two sqlite connections (two `Store` handles on one
        // on-disk db, NO shared in-process mutex) genuinely overlap. This is the case an
        // in-process single-`Store` test cannot reach - one `Mutex<Connection>` serializes
        // its appends so they never contend - which is exactly why the sequential tests
        // above give false confidence. Here the worker self-reports SUCCESS via the plain
        // path while the courier fires `--if-absent --error`, round-synchronized so they
        // collide on the same id every round.
        //
        // Invariants (records atomically: no lost, no orphan, no hard-fail):
        //   - the courier's `--if-absent` never hard-fails (no cross-connection lock error);
        //   - the worker's self-report is never dropped;
        //   - every id ends with the worker's SUCCESS, never force-failed by the courier -
        //     because whenever the courier's `--error` could land, the worker's later
        //     success supersedes it, and whenever the success landed first the courier
        //     re-checks and no-ops.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.db");
        let path = path.to_str().unwrap().to_string();

        // Two connections on one file, opened up front so we race only the appends.
        let worker_store = std::sync::Arc::new(Store::open(&path).unwrap());
        let courier_store = std::sync::Arc::new(Store::open(&path).unwrap());

        const ROUNDS: usize = 40;
        let ids: Vec<String> = (0..ROUNDS).map(|i| format!("u/implementer#{i}")).collect();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        let w_ids = ids.clone();
        let w_barrier = barrier.clone();
        let w_store = worker_store.clone();
        let worker = std::thread::spawn(move || {
            let mut errs = 0usize;
            for id in &w_ids {
                w_barrier.wait();
                if record_result(w_store.as_ref(), &SpawnResult::ok(id, "self-reported")).is_err() {
                    errs += 1;
                }
            }
            errs
        });

        let c_ids = ids.clone();
        let c_barrier = barrier.clone();
        let c_store = courier_store.clone();
        let courier = std::thread::spawn(move || {
            let mut errs = 0usize;
            for id in &c_ids {
                c_barrier.wait();
                if record_result_if_absent(c_store.as_ref(), &SpawnResult::failed(id, "died"))
                    .is_err()
                {
                    errs += 1;
                }
            }
            errs
        });

        let worker_errs = worker.join().unwrap();
        let courier_errs = courier.join().unwrap();
        assert_eq!(
            courier_errs, 0,
            "the courier's --if-absent must never hard-fail on a cross-connection race"
        );
        assert_eq!(
            worker_errs, 0,
            "the worker's self-report must never be dropped on a cross-connection race"
        );

        let events = worker_store
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap();
        for id in &ids {
            let got = result_of(&events, id)
                .unwrap()
                .unwrap_or_else(|| panic!("{id} must have a recorded result (no orphan, no lost)"));
            assert!(
                !got.is_error(),
                "{id} must end with the worker's success, never force-failed by the courier"
            );
            assert_eq!(
                got.output, "self-reported",
                "the self-report must stand for {id}"
            );
        }
    }

    #[test]
    fn a_result_does_not_count_as_a_parked_request() {
        // The request and result halves share the stream but are distinct facts: a
        // result must not make `recorded`/`is_recorded` (which count REQUESTS) match.
        let store = Store::open(":memory:").unwrap();
        record_result(&store, &SpawnResult::ok("u/implementer#0", "done")).unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
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
        let store = Store::open(":memory:").unwrap();
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                std::slice::from_ref(&Event::new("UnitStarted", br#"{"id":"u"}"#.to_vec())),
            )
            .unwrap();
        park(
            &store,
            &SpawnRequest::new("u", "implement", ROLE_IMPLEMENTER, 0, "do it"),
        )
        .unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
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
        let store = Store::open(":memory:").unwrap();
        let old = SpawnRequest::new("plan", "plan", ROLE_IMPLEMENTER, 0, "plan it");
        park(&store, &old).unwrap();
        record_result(&store, &SpawnResult::ok(&old.id, "planned")).unwrap();

        park(
            &store,
            &SpawnRequest::new("b", "implement", ROLE_IMPLEMENTER, 0, "b"),
        )
        .unwrap();
        park(
            &store,
            &SpawnRequest::new("a", "implement", ROLE_IMPLEMENTER, 0, "a"),
        )
        .unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
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
    fn step_rerun_reprints_unanswered_spawns_so_a_killed_step_orphans_nothing() {
        // Disposable step processes (spec 04): a step killed after parking but before
        // printing must not orphan its spawns. A later step's wave re-prints every
        // spawn still awaiting a result, so a relaunched driver resumes the in-flight
        // wave; the answered spawn does not reappear.
        let store = Store::open(":memory:").unwrap();
        let a = SpawnRequest::new("a", "implement", ROLE_IMPLEMENTER, 0, "a");
        let b = SpawnRequest::new("b", "implement", ROLE_IMPLEMENTER, 0, "b");
        park(&store, &a).unwrap();
        park(&store, &b).unwrap();

        // `a` was answered; `b`'s wave JSON never reached a driver (killed step).
        record_result(&store, &SpawnResult::ok(&a.id, "did a")).unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
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
        record_result(&store, &SpawnResult::ok(&b.id, "did b")).unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let step = step_result(&events).unwrap();
        assert!(step.wave.is_empty(), "nothing awaits a result");
        assert!(step.done, "every recorded spawn now has a result");
    }

    #[test]
    fn step_on_an_empty_log_is_done_with_an_empty_wave() {
        // No spawn was ever parked: vacuously done, empty wave (nothing left to run).
        let store = Store::open(":memory:").unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let step = step_result(&events).unwrap();
        assert!(step.wave.is_empty());
        assert!(step.done);
    }

    #[test]
    fn step_serializes_to_a_wave_array_and_a_done_bool() {
        // The JSON `rigger step` prints: {"wave":[<SpawnRequest>...],"done":<bool>}.
        let store = Store::open(":memory:").unwrap();
        park(
            &store,
            &SpawnRequest::new("u", "implement", ROLE_IMPLEMENTER, 0, "do it"),
        )
        .unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
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
    }

    #[test]
    fn prompt_for_returns_persona_and_task_by_spawn_id() {
        let store = Store::open(":memory:").unwrap();
        let mut req = SpawnRequest::new("u", "implement", ROLE_IMPLEMENTER, 0, "do the task");
        req.system_prompt = "you are the implementer".into();
        park(&store, &req).unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
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
