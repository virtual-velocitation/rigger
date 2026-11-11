//! Design-intent doc extraction (spec 29b): reads the reference architecture, `architecture.md`,
//! the addenda, load-bearing decisions, spec-shape / loop-discipline rules, and inline `# WHY:`
//! rationale into the concepts and links the code store's extractor carries on a file's facts,
//! which the code view composes into `design-doc` / `arch-decision` / `handbook-rule` /
//! `rationale` nodes. Design intent is the design half of the one graph (spec 29a is the code
//! half), so an agent grounded on a subsystem reaches the INTENT behind it, not just its code and
//! prior decisions.
//!
//! The extraction is feature-gated behind `symbols`, mirroring the code extractor: the node-kind
//! and relation vocabulary (`contextgraph`) is always compiled, while producing concepts from a
//! document is extraction and stays out of the light lane.

pub mod extract;
pub mod model;
