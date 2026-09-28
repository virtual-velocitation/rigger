//! Grounding gives each agent only the context it needs. The `Grounder` port lives in
//! `rigger-domain` and the grounders in `rigger-grounder`; this module re-exports both under the
//! historical `rigger::grounder` path, beside the workflow-definition pass that stays in the root
//! until the config's store half leaves it.

pub use rigger_grounder::grounder::*;

// The workflow-DEFINITION grounding axis (spec 92 criterion 2, THE WHOLE PRODUCT IS COVERED): a
// `.rigger/workflow.yml` extraction pass that lowers its stages, gates and agent roles into the
// SAME DocConceptExtracted/DocLinkExtracted events `design` above emits (never a second
// entity/edge-fold authority; see [`crate::contextgraph`]'s fold arm doc). Gated behind
// `symbols` exactly like `design` - it is the EMIT half; the always-compiled fold folds a
// workflow-definition log with this pass absent, so the light lane never links it.
#[cfg(feature = "symbols")]
pub mod workflowdef;
