# Phase 1: Event Store - Implementation Plan

> **For agentic workers:** implement task-by-task, TDD, committing each task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the KurrentDB-shaped `EventStore` trait, an embedded SQLite adapter, and a backend-agnostic contract suite that any store (SQLite now, KurrentDB in Phase 2) must pass.

**Architecture:** An `eventstore` module (`src/eventstore/mod.rs`) defines the trait and value types. An `eventstore::sqlite` module implements it over bundled `rusqlite` (a single SQLite file, no external service). An `eventstore::contract` module exposes `assert_contract(store)` so every backend proves identical behavior; that shared suite is the test-proxy fidelity the architecture calls for.

**Tech Stack:** Rust (edition 2021), `rusqlite` with the `bundled` feature (SQLite compiled in, no system dependency), the standard `#[test]` harness, `tempfile` for temp DBs in dev-tests.

## Global Constraints

- `rusqlite` with `bundled` so SQLite is compiled into the crate (no system libsqlite dependency); the default build pulls no async runtime and no network client.
- The trait mirrors KurrentDB primitives (streams, a global `$all` order, optimistic-concurrency append, catch-up subscriptions) so the Phase 2 KurrentDB adapter is a drop-in behind the same trait.
- TDD throughout: the contract suite is written against the trait; the SQLite adapter makes it pass.
- Grounded on `docs/architecture.md` sections 5.1 (the trait + types) and 13 (Phase 1 "done when": the contract suite passes against `sqlite`).

## File structure

- `src/eventstore/mod.rs` - the trait + value types: `Event`, `Position`, `Direction`, `ExpectedRevision`, `Filter`, `Error`, `EventStore`, `Subscription`.
- `src/eventstore/sqlite.rs` - the SQLite adapter: `Store::open`, plus the `EventStore` impl (`append`, `read_stream`, `read_all`, `subscribe_all`).
- `src/eventstore/contract.rs` - `assert_contract(store: &dyn EventStore)`, the backend-agnostic suite.
- `#[cfg(test)]` modules in `sqlite.rs` (and `namespace.rs`) call `assert_contract` against a fresh store, plus any sqlite-specific tests.

## Trait (the contract every backend implements)

```rust
// src/eventstore/mod.rs
pub type Position = u64; // global $all order, assigned by the store on append

pub enum Direction { Forward, Backward }

// Optimistic-concurrency expectation: any version, no stream yet, or an exact event count.
pub enum ExpectedRevision { Any, NoStream, Exact(u64) }

// A read/subscription filter over the global log (a stream-name prefix).
pub struct Filter { pub stream_prefix: Option<String> }

pub struct Event {
    pub id: String,              // a fresh UUID per event (Event::new assigns it)
    pub type_: String,
    pub data: Vec<u8>,           // the opaque (usually JSON) payload
    pub recorded_at: SystemTime, // when the event was created / ingested
    pub position: Position,      // store-assigned global order
}

pub enum Error {
    Conflict { stream: String }, // a failed optimistic-concurrency expectation
    Backend(String),             // any backend error
}

pub trait EventStore: Send + Sync {
    fn append(&self, stream: &str, expected: ExpectedRevision, events: &[Event])
        -> Result<Position, Error>;
    fn read_stream(&self, stream: &str, from: Position, dir: Direction)
        -> Result<Vec<Event>, Error>;
    fn read_all(&self, from: Position, dir: Direction, filter: &Filter)
        -> Result<Vec<Event>, Error>;
    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error>;
}

// Subscription is a concrete catch-up handle (not a trait): a background thread feeds
// events onto an mpsc channel; callers drain it, and dropping it stops the feed.
pub struct Subscription { /* rx, stop flag, join handle */ }
impl Subscription {
    pub fn recv(&self) -> Option<Event>;                       // block for the next event
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event>;
    pub fn try_recv(&self) -> Option<Event>;                   // non-blocking
}
```

Note the shape that diverged from the original Go sketch: there is **no per-stream `Revision`
type** — events carry only the global `Position`, and `read_stream` reads from a `Position`;
`Event` carries **no `Stream`/`Meta`/`ValidFrom` field** (the stream is an `append` argument,
the bi-temporal validity lives in the context-graph projection, not the raw event); the
expectation is an enum (`Any | NoStream | Exact(u64)`) rather than sentinel integers; and the
conflict error is `Error::Conflict { stream }` (no expected/actual fields). There is no
`Close` — the SQLite handle closes on drop.

## SQLite schema

```sql
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
CREATE TABLE IF NOT EXISTS events (
  position    INTEGER PRIMARY KEY AUTOINCREMENT, -- global $all order
  stream      TEXT    NOT NULL,
  type        TEXT    NOT NULL,
  id          TEXT    NOT NULL,
  data        BLOB    NOT NULL,
  recorded_at INTEGER NOT NULL                   -- unix nanos
);
CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream);
```

`append` runs against a single connection held behind a `Mutex` (so concurrent appenders serialize rather than racing on a SQLite lock-upgrade): it reads `SELECT COUNT(*) FROM events WHERE stream = ?` as the stream's current event count, compares it against the expectation (`Any` always passes; `NoStream` requires count 0; `Exact(v)` requires count == v); on mismatch it returns `Error::Conflict { stream }`; else it inserts each event (the `position` auto-increments). It returns the `Position` of the last inserted event. The connection mutex is the concurrency backstop — there is no per-stream `revision` column or unique index.

`subscribe_all` first replays every matching event with `position >= from` (history), then a background thread polls `position > last_seen` on a short interval and pushes new events onto the channel; the `Subscription`'s `Drop` sets the stop flag and joins the thread. The connection is shared via `Arc<Mutex<Connection>>` so the polling thread reads the same database the writers append to.

## Tasks

### Task 1: Crate deps + trait + types

**Files:** Create `src/eventstore/mod.rs`; modify `Cargo.toml` (add `rusqlite` with the `bundled` feature, `uuid`, `thiserror`).

- [ ] Add the dependencies in `Cargo.toml`: `rusqlite = { version = "0.32", features = ["bundled"] }`, `uuid` (v4), `thiserror`; `cargo build`.
- [ ] Write `src/eventstore/mod.rs` with the trait + types above (no behavior beyond `Event::new`). Add `pub mod eventstore;` in `src/lib.rs`.
- [ ] Write a tiny `#[cfg(test)]` test that constructs an `Event` via `Event::new`, formats `Error::Conflict { stream }`, and asserts the message contains the stream; `cargo test eventstore` and watch it pass (pure value code).
- [ ] Commit: `feat(eventstore): trait and value types`.

### Task 2: SQLite open + append with optimistic concurrency

**Files:** Create `src/eventstore/sqlite.rs` (with a `#[cfg(test)] mod tests`).

- [ ] Write a failing test: open `Store::open(":memory:")`, `append("s", NoStream, &[e1, e2])` returns a `Position` and no error; a second `append("s", NoStream, &[e3])` returns `Err(Error::Conflict { .. })`; `append("s", Exact(2), &[e3])` succeeds. Run it, watch it fail to compile/pass.
- [ ] Implement `Store::open(path)` (creating the schema) and `append` (the serialized count/check/insert above). Run the test, watch it pass.
- [ ] Commit: `feat(eventstore/sqlite): open and append with optimistic concurrency`.

### Task 3: read_stream + read_all (ordering + filter)

**Files:** modify `src/eventstore/sqlite.rs`.

- [ ] Failing test: after appending to streams "a" and "b" interleaved, `read_stream("a", 0, Forward)` returns a's events in `Position` order; `read_all(0, Forward, &Filter::default())` returns ALL events in global `Position` order across both streams; a `Filter { stream_prefix: Some("a") }` narrows it; `Backward` reverses. Run, watch fail.
- [ ] Implement `read_stream` (`WHERE stream=? AND position>=? ORDER BY position`) and `read_all` (`WHERE position>=? [AND stream LIKE prefix||'%'] ORDER BY position`), honoring `Direction`. Run, watch pass.
- [ ] Commit: `feat(eventstore/sqlite): read_stream and read_all`.

### Task 4: subscribe_all (catch-up: replay then live)

**Files:** modify `src/eventstore/sqlite.rs`.

- [ ] Failing test: append e1,e2; `subscribe_all(0, &Filter::default())`; `recv` e1,e2 off the subscription (history); then `append` e3 and assert e3 arrives via `recv_timeout` within a deadline; dropping the `Subscription` stops the feed. Run, watch fail.
- [ ] Implement `subscribe_all`: spawn a thread that first drains `position >= from`, then polls `position > last_seen` on a short interval, pushing onto an mpsc channel; `Subscription::new(rx, stop, handle)` ties them together so `Drop` stops and joins. Run, watch pass (use `recv_timeout` in the test, not a sleep-assert).
- [ ] Commit: `feat(eventstore/sqlite): catch-up subscribe_all`.

### Task 5: The shared contract suite

**Files:** Create `src/eventstore/contract.rs`; call it from the `#[cfg(test)] mod tests` in `sqlite.rs`.

- [ ] Write `assert_contract(store: &dyn EventStore)` consolidating the behaviors: append+read ordering, optimistic-concurrency conflict (NoStream), catch-up replay-then-live. Gate it `#[cfg(test)]` (or feature-gated) so it ships with the tests.
- [ ] In `sqlite.rs` tests, add a test calling `crate::eventstore::contract::assert_contract(&Store::open(":memory:").unwrap())`. `cargo test`, watch all pass.
- [ ] Commit: `test(eventstore): backend-agnostic contract suite, sqlite passes it`.

## Done when

`cargo test` is green, the contract suite (`assert_contract`) passes against the SQLite backend, and the crate still builds (the binary is still a stub until Phase 5). CI on `main` (the `build-test` job: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`) stays green.
