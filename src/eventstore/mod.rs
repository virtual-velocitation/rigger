//! The append-only, bi-temporal event store: an immutable log of facts the ledger
//! and context graph are projected from. `EventStore` is the port; `sqlite` is the
//! default adapter. The trait mirrors KurrentDB's primitives - per-stream append
//! with optimistic concurrency, a global $all order, per-stream revisions, and
//! catch-up subscriptions - so a backend swaps without changing the rest of Rigger.
//!
//! The port and `Event` live in `rigger-domain`; the adapters and the connection-string
//! redaction live in `rigger-store-sqlite`. This module re-exports both under the historical
//! `rigger::eventstore` path.

pub use rigger_store_sqlite::eventstore::*;
