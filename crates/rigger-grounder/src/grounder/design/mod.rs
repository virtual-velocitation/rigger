//! Design-intent doc extraction (spec 29b): lowers the reference architecture, `architecture.md`,
//! the addenda, load-bearing decisions, spec-shape / loop-discipline rules, and inline `# WHY:`
//! rationale into `DocConceptExtracted` events, which the always-compiled context-graph fold
//! ingests into `design-doc` / `arch-decision` / `handbook-rule` / `rationale` nodes. Design intent
//! thus becomes a rebuildable projection over the event log - the design half of the one graph
//! (spec 29a is the code half), so an agent grounded on a subsystem reaches the INTENT behind it,
//! not just its code and prior decisions.
//!
//! This is the EMIT half and is feature-gated behind `symbols`, mirroring the code extractor: the
//! always-compiled fold (`contextgraph::sqlite`) and the node-kind / event / payload vocabulary
//! (`contextgraph`) fold a design-intent log with this extraction pass absent (the light lane),
//! while PRODUCING that log is extraction and stays out of the light lane.

pub mod events;
pub mod extract;
pub mod model;
