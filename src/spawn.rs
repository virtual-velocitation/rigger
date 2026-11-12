//! Spawn requests: the unit of work a driver launches or answers from the log.
//!
//! Each request carries a DETERMINISTIC id derived from its position in the run
//! structure (unit id + stage/role + attempt) - never wall clock, randomness, or an
//! in-memory counter - so a conductor process that re-runs over recorded history
//! computes the SAME id for the SAME spawn and matches a recorded result back to the
//! call that produced it, across process boundaries (§4, spec 04).
//!
//! A request also carries everything the agent host needs to actually run
//! the agent: the grounded task `prompt`, the agent's `system_prompt` (its persona /
//! role instructions), its `model` alias, its granted `tools`, its working `dir`,
//! and the `unit` id + `stage` for the per-unit progress label the driver builds.
//!
//! A driver that reaches an UNRECORDED spawn at the frontier PARKS the call: the
//! request is persisted to the run's event log as a [`TYPE_SPAWN_REQUESTED`] event
//! (this module owns that serialization). A driver answers an already-recorded spawn
//! from the log, and the spawn-budget breaker counts those same DISTINCT requests
//! (via [`recorded`], deduped by id - a re-parked id is never double-counted) - so both
//! derive from the log rather than process memory, and the breaker binds across every
//! conductor process the run spans.

pub use rigger_domain::spawn::*;
