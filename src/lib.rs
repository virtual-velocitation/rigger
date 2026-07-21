//! Rigger: a config-driven, event-sourced multi-agent dev-loop harness.
//!
//! Clean Architecture: the ports are traits (EventStore, ...), the adapters live
//! beside them, and the conductor is the top-level use case depending only on
//! ports. It generalizes a proven internal multi-agent dev-loop harness into a
//! standalone, config-driven product.

pub mod blocker;
pub mod canary;
pub mod conductor;
pub mod config;
pub mod contextgraph;
pub mod dash;
/// The sleep-phase consolidation distiller (spec 27): a rebuildable projection that folds
/// OLDER-THAN-CURRENT-RUN findings/decisions into per-file digest nodes so cross-run graph
/// growth stays bounded automatically. Modeled on [`playbooks`]; reads existing events only.
pub mod distiller;
/// The self-documenting discipline pipeline (spec 20, unit 1): a typed, code-derived
/// context rendered into the `using-rigger` skill and the handbook discipline chapter,
/// so the operating discipline stays in lock-step with the code the binary runs on.
pub mod docs;
pub mod driver;
pub mod eventstore;
pub mod failure;
pub mod gate;
pub mod grounder;
pub mod hooks;
pub mod ledger;
pub mod liveness;
pub mod mcpserver;
pub mod metrics;
/// Points `ort` at a CUDA-enabled ONNX Runtime `.so` to `dlopen` (`load-dynamic`), so
/// the turbovec grounder embeds on the GPU with no user-set env. Only meaningful when
/// `ort` is compiled in, hence gated on the `turbovec` feature.
#[cfg(feature = "turbovec")]
pub mod ort_runtime;
#[cfg(feature = "turbovec")]
pub mod ort_teardown;
pub mod playbooks;
pub mod progress;
pub mod reap;
pub mod run;
pub mod safety;
pub mod sidecar;
pub mod spawn;
pub mod spec;
pub mod worktree;

/// Spec 16 unit 2 - the partitioning + routing SAFETY EVAL (architecture 5.5.8). A GATE, not a
/// runtime surface: it is compiled ONLY under `cfg(test)`, adds no API and no event, and its
/// quantified arms are feature-gated behind `symbols` internally. It authorizes unit 3 wiring
/// `blast_radius` into the conductor by proving the safe view is a grep superset and that the
/// safe-superset partitioning retains parallelism and a non-collapsed tier split.
#[cfg(test)]
mod blast_radius_eval;
