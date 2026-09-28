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

pub use rigger_domain::blocker;
pub use rigger_domain::canary;
/// The machine-wide build concurrency budget (spec 65): a flock-based slot directory that
/// caps how many actual compiler invocations run at once, across every rigger process on
/// the machine. Wired at the gate-build call site ([`crate::gate::ExecRunner::run`]) only -
/// slots bound builds, never agents.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_process::budget;
/// The write half of [`canary`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod canary_store;
pub use rigger_domain::community;
pub mod concepts;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_conductor::conductor;
pub use rigger_config_files::config;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_config_files::config_store;
/// The console's view models and ONE FOLD (spec 93, criterion 4): the pure
/// reconstruction of unit statuses, current-blocker lines, the needs-you dock and
/// the statusline from a recorded event stream, shared between `rigger status`
/// and the Mission Control console page (specs 94-98). Always compiled (`core`):
/// no `rusqlite`, `tokio`, `std::fs`, `std::process`, `std::net` or clock read.
pub use rigger_console::console;
pub mod contextgraph;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod dash;
/// The self-documenting discipline pipeline (spec 20, unit 1): a typed, code-derived
/// context rendered into the `using-rigger` skill and the handbook discipline chapter,
/// so the operating discipline stays in lock-step with the code the binary runs on.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod docs;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_driver::driver;
pub mod eventstore;
pub use rigger_domain::failure;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_gates_shell::gate;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod grounder;
pub use rigger_domain::instructions;
pub use rigger_domain::ledger;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_driver::hooks;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_driver::liveness;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_grounder::ingest;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod mcpserver;
pub use rigger_domain::metrics;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_process::parallel;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod playbooks;
pub mod progress;
pub use rigger_domain::run;
pub use rigger_domain::safety;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_driver::sidecar;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_process::reap;
/// The write half of [`progress`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_store_sqlite::progress_store;
/// Machine-global instance registry (spec 50): credential-free discovery metadata so a single
/// machine-level dash can find every local project's runs (and any configured shared store)
/// without a coordination protocol. Discovery only - never a source of truth, never a credential.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_store_sqlite::registry;
/// The write half of [`run`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_store_sqlite::run_store;
pub mod spawn;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_domain::spec;
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_domain::watch;
/// The driver-independent watchdog (spec 69, criterion 2): `rigger watch`'s pure
/// domain core - the five `rigger-watch-a-run` signals plus a store-integrity check,
/// folded from already-gathered inputs into one line per anomaly, with in-process
/// streaming dedup. Never touches the driver.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_process::subprocess;
/// The write half of [`spawn`] (spec 93, criterion 1): see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_store_sqlite::spawn_store;
/// The one opener every SQLite store connection goes through: see that module's own doc.
#[cfg(any(feature = "store", not(feature = "core")))]
pub use rigger_store_sqlite::sqlite;
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod worktree;

/// Parameterised tests: one shared case helper, one generated `#[test]` per named case.
pub use rigger_domain::test_cases;

/// Spec 16 unit 2 - the partitioning + routing SAFETY EVAL (architecture 5.5.8). A GATE, not a
/// runtime surface: it is compiled ONLY under `cfg(test)`, adds no API and no event, and its
/// quantified arms are feature-gated behind `symbols` internally. It authorizes unit 3 wiring
/// `blast_radius` into the conductor by proving the safe view is a grep superset and that the
/// safe-superset partitioning retains parallelism and a non-collapsed tier split.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
mod blast_radius_eval;

/// The fixtures an inline `#[cfg(test)]` module shares with the integration suites: ONE source
/// tree, `tests/common/fixtures/`, compiled into each test crate that needs it (the integration
/// suites through `tests/common/`, this library's and the binary's unit tests through a
/// `#[path]` module), so a fixture both sides use is defined exactly once. The fixtures name the
/// library as `rigger::...` - the dependency's name in every other test crate - which
/// `extern crate self as rigger` makes resolve here too.
#[cfg(test)]
extern crate self as rigger;
#[cfg(test)]
#[path = "../tests/common/fixtures/mod.rs"]
mod test_support;
