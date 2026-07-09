//! Rigger: a config-driven, event-sourced multi-agent dev-loop harness.
//!
//! Clean Architecture: the ports are traits (EventStore, ...), the adapters live
//! beside them, and the conductor is the top-level use case depending only on
//! ports. It generalizes a proven internal multi-agent dev-loop harness into a
//! standalone, config-driven product.

pub mod canary;
pub mod conductor;
pub mod config;
pub mod contextgraph;
pub mod dash;
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
pub mod progress;
pub mod run;
pub mod safety;
pub mod sidecar;
pub mod spawn;
pub mod spec;
pub mod worktree;
