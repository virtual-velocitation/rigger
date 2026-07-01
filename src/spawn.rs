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
//! breaker counts them - so both derive from the log rather than process memory.
//!
//! This is DISTINCT from `driver::workflow::SpawnRequest`, the in-process MCP
//! driver's wire type: that path (the shim) keeps working unchanged, while this
//! vocabulary is what the stepwise `rigger step` / `rigger result` surface persists.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::conductor::STREAM;
use crate::eventstore::{Error, Event, EventStore, ExpectedRevision, Position};

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
    let ev = req
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize spawn request {}: {e}", req.id)))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::Direction;

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
}
