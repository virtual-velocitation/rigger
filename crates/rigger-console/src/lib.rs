//! Rigger's console delivery, ring 4 of the workspace: the console's ONE FOLD (spec 93,
//! criterion 4) - the pure reconstruction of unit statuses, current-blocker lines, the needs-you
//! dock and the statusline from a recorded event stream - and the code map, shared by
//! `rigger status` and the Mission Control console page. Always compiled (`core`): no `rusqlite`,
//! `tokio`, `std::fs`, `std::process`, `std::net` or clock read. The root `rigger` crate
//! re-exports it under its historical `rigger::console` path, and `console-core` compiles it to
//! WebAssembly.

pub mod console;

// The modules the moved code names by their historical `crate::` paths.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
use rigger_domain::retention;
#[cfg(test)]
use rigger_domain::test_cases;
use rigger_domain::{blocker, contextgraph, eventstore, ledger};
mod spawn {
    /// The minimal request the tests build, defined once with the shared fixtures.
    #[cfg(test)]
    pub(crate) use crate::test_support::test_request;
    pub use rigger_domain::spawn::*;
}

/// The fixtures an inline `#[cfg(test)]` module shares with the root crate's tests, compiled
/// here from the same files. They name the crate as `rigger::...`, which
/// `extern crate self as rigger` makes resolve to this crate, whose modules sit at the same
/// paths the root facade re-exports them under.
#[cfg(test)]
extern crate self as rigger;
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/events.rs"]
mod event_fixtures;
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/spawn.rs"]
mod spawn_fixtures;
#[cfg(test)]
mod test_support {
    pub use crate::event_fixtures::*;
    pub use crate::spawn_fixtures::*;
}
