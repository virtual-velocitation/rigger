//! Rigger: a config-driven, event-sourced multi-agent dev-loop harness.
//!
//! Clean Architecture: the ports are traits (EventStore, ...), the adapters live
//! beside them, and the conductor is the top-level use case depending only on
//! ports. It generalizes a proven internal multi-agent dev-loop harness into a
//! standalone, config-driven product.

// THE FEATURE SPLIT (spec 93, criterion 1): a `core` module is either wholly listed
// below with NO cfg gate (the pure subset: model, queries, folds - none of them touch a
// file, a socket, a process or the clock) or wholly behind
// `#[cfg(any(feature = "store", not(feature = "core")))]` (every module that does). The
// predicate is `store OR NOT core`, not a bare `feature = "store"` check, so that a bare
// `--no-default-features` (store off, core also off) still builds every store-gated
// module - see the `store` feature's own doc in Cargo.toml for why. Never gated function
// by function: where a file mixed both (`spawn.rs`, `run.rs`, `progress.rs`), the impure
// half moved to its own `_store` file (`spawn_store`, `run_store`, `progress_store`).

/// Declares one `#[test]` per case of a table-driven test family: each case names its test
/// (plus any attributes, e.g. its doc or `#[should_panic]`) and gives the expression that runs
/// it - normally one call into the family's shared case body - so a family of same-shaped
/// tests has ONE definition of its body instead of a copy per test. Defined before every
/// module so each test module can invoke it by name.
#[cfg(test)]
macro_rules! test_cases {
    ($($(#[$attr:meta])* $name:ident => $body:expr;)+) => {
        $(
            $(#[$attr])*
            #[test]
            fn $name() {
                $body;
            }
        )+
    };
}

pub mod blocker;
/// The machine-wide build concurrency budget (spec 65): a flock-based slot directory that
/// caps how many actual compiler invocations run at once, across every rigger process on
/// the machine. Wired at the gate-build call site ([`crate::gate::ExecRunner::run`]) only -
/// slots bound builds, never agents.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod budget;
pub mod canary;
/// The write half of [`canary`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod canary_store;
/// Deterministic coupling-community detection (spec 53, the CODE lens): the offline pass that
/// groups code entities and files by how densely they call and reference one another, regardless of
/// directory, and records the result as `CommunityAssigned` events the always-compiled fold turns
/// into `IN_COMMUNITY` membership edges. Always compiled and proven in both feature lanes.
pub mod community;
pub mod concepts;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod conductor;
pub mod config;
/// The write half of [`config`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod config_store;
/// The console's view models and ONE FOLD (spec 93, criterion 4): the pure
/// reconstruction of unit statuses, current-blocker lines, the needs-you dock and
/// the statusline from a recorded event stream, shared between `rigger status`
/// and the Mission Control console page (specs 94-98). Always compiled (`core`):
/// no `rusqlite`, `tokio`, `std::fs`, `std::process`, `std::net` or clock read.
pub mod console;
pub mod contextgraph;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod dash;
/// The self-documenting discipline pipeline (spec 20, unit 1): a typed, code-derived
/// context rendered into the `using-rigger` skill and the handbook discipline chapter,
/// so the operating discipline stays in lock-step with the code the binary runs on.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod docs;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod driver;
pub mod eventstore;
pub mod failure;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod gate;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod grounder;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod hooks;
/// Project-source ingest into the context graph (spec 45): the ONE walk-and-content-key
/// authority both the live run and the standalone `rigger graph build` entry share, so the
/// content key an event is deduped under can never drift between the two ingest entries.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod ingest;
/// Instruction injection: the built-in engineering law and the operator's
/// `.rigger/instructions/*.md` layered into every spawned agent's system prompt.
pub mod instructions;
pub mod ledger;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod liveness;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod mcpserver;
pub mod metrics;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod parallel;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod playbooks;
pub mod progress;
/// The write half of [`progress`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod progress_store;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod reap;
/// Machine-global instance registry (spec 50): credential-free discovery metadata so a single
/// machine-level dash can find every local project's runs (and any configured shared store)
/// without a coordination protocol. Discovery only - never a source of truth, never a credential.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod registry;
pub mod run;
/// The write half of [`run`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod run_store;
pub mod safety;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod sidecar;
pub mod spawn;
/// The write half of [`spawn`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod spawn_store;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod spec;
/// The driver-independent watchdog (spec 69, criterion 2): `rigger watch`'s pure
/// domain core - the five `rigger-watch-a-run` signals plus a store-integrity check,
/// folded from already-gathered inputs into one line per anomaly, with in-process
/// streaming dedup. Never touches the driver.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod watch;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod worktree;

/// Spec 16 unit 2 - the partitioning + routing SAFETY EVAL (architecture 5.5.8). A GATE, not a
/// runtime surface: it is compiled ONLY under `cfg(test)`, adds no API and no event, and its
/// quantified arms are feature-gated behind `symbols` internally. It authorizes unit 3 wiring
/// `blast_radius` into the conductor by proving the safe view is a grep superset and that the
/// safe-superset partitioning retains parallelism and a non-collapsed tier split.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod blast_radius_eval;
