# Phase 1: Event Store - Implementation Plan

> **For agentic workers:** implement task-by-task, TDD, committing each task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the KurrentDB-shaped `EventStore` interface, an embedded SQLite implementation, and a backend-agnostic contract test suite that any store (SQLite now, KurrentDB in Phase 2) must pass.

**Architecture:** An `eventstore` package defines the interface and value types. An `eventstore/sqlite` package implements it over `modernc.org/sqlite` (pure Go, cgo-free, single file). An `eventstore/eventstoretest` package exposes `RunContract(t, factory)` so every backend proves identical behavior; that shared suite is the test-proxy fidelity the architecture calls for.

**Tech Stack:** Go 1.26.4, `modernc.org/sqlite` (pure-Go SQLite, no cgo), the standard `testing` package.

## Global Constraints

- Pure Go, cgo-free, so the binary stays a single static cross-compilable artifact (no `mattn/go-sqlite3`).
- The interface mirrors KurrentDB primitives (streams, a global `$all` order, optimistic-concurrency append, catch-up subscriptions) so the Phase 2 KurrentDB adapter is a drop-in behind the same interface.
- TDD throughout: the contract suite is written against the interface; the SQLite impl makes it pass.
- Grounded on `docs/architecture.md` sections 5.1 (the interface + types) and 13 (Phase 1 "done when": the contract tests pass against `sqlite`).

## File structure

- `eventstore/eventstore.go` - interface + value types: `Event`, `Position`, `Revision`, `Direction`, `ExpectedRevision`, `ConflictError`, `EventStore`, `Subscription`.
- `eventstore/sqlite/sqlite.go` - the SQLite impl: `Open`, `Append`, `ReadStream`, `ReadAll`, `SubscribeAll`, `Close`.
- `eventstore/eventstoretest/contract.go` - `RunContract(t *testing.T, factory func(t *testing.T) eventstore.EventStore)`.
- `eventstore/sqlite/sqlite_test.go` - runs `RunContract` against a fresh SQLite store, plus any sqlite-specific tests.

## Interface (the contract every backend implements)

```go
package eventstore

type Position uint64 // global $all order, 1-based, assigned on append
type Revision int64  // per-stream order, 0-based; an empty stream is at NoStream

type Direction int
const ( Forward Direction = iota; Backward )

type ExpectedRevision int64
const (
    Any      ExpectedRevision = -2 // no concurrency check
    NoStream ExpectedRevision = -1 // the stream must not yet exist
    // >= 0: the stream's current last-revision must equal this exactly
)

type Event struct {
    ID         string            // caller idempotency key, unique within a stream
    Stream     string            // set by Append
    Type       string
    Data       []byte
    Meta       map[string]string
    ValidFrom  time.Time         // caller-supplied: when the fact became true (bi-temporal)
    RecordedAt time.Time         // store-stamped: when ingested (bi-temporal)
    Position   Position          // store-assigned global order
    Revision   Revision          // store-assigned per-stream order
}

type ConflictError struct {
    Stream   string
    Expected ExpectedRevision
    Actual   Revision // the stream's real current revision (NoStream if absent)
}
func (e *ConflictError) Error() string

type EventStore interface {
    Append(ctx context.Context, stream string, expected ExpectedRevision, events ...Event) (Position, error)
    ReadStream(ctx context.Context, stream string, from Revision, dir Direction) ([]Event, error)
    ReadAll(ctx context.Context, from Position, dir Direction) ([]Event, error)
    SubscribeAll(ctx context.Context, from Position) (Subscription, error)
    Close() error
}

type Subscription interface {
    Events() <-chan Event // historical (from `from`) then live, in global order
    Err() error           // terminal error after Events() closes
    Close() error
}
```

## SQLite schema

```sql
CREATE TABLE IF NOT EXISTS events (
  position    INTEGER PRIMARY KEY AUTOINCREMENT, -- global $all order
  stream      TEXT    NOT NULL,
  revision    INTEGER NOT NULL,                  -- per-stream 0-based
  id          TEXT    NOT NULL,
  type        TEXT    NOT NULL,
  data        BLOB,
  meta        TEXT,                              -- JSON object
  valid_from  TEXT    NOT NULL,                  -- RFC3339Nano
  recorded_at TEXT    NOT NULL,                  -- RFC3339Nano
  UNIQUE(stream, revision),                      -- per-stream ordering + race backstop
  UNIQUE(stream, id)                             -- idempotency key
);
CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream, revision);
```

`Append` runs in a transaction: read `MAX(revision)` for the stream (the current last revision, or `NoStream` if none); compare against `expected`; if mismatch, return `*ConflictError`; else insert each event at `last+1, last+2, ...`. The `UNIQUE(stream, revision)` constraint is the race backstop (two concurrent appends with the same expected revision: one commits, the other hits the constraint and is mapped to `*ConflictError`). Return the `Position` of the last inserted event.

`SubscribeAll` first replays every event with `position >= from` (history), then polls `position > lastSeen` on a short interval and pushes new events onto the channel until `Close` or context cancel.

## Tasks

### Task 1: Module deps + interface + types

**Files:** Create `eventstore/eventstore.go`; modify `go.mod` (add `modernc.org/sqlite`).

- [ ] Add the dependency: `go get modernc.org/sqlite@latest`, then `go mod tidy`.
- [ ] Write `eventstore/eventstore.go` with the interface + types above (no behavior).
- [ ] Write a tiny test that constructs an `Event`, a `*ConflictError`, and asserts `ConflictError.Error()` contains the stream + expected + actual; run `go test ./eventstore/...` and watch it pass (it is pure value code).
- [ ] Commit: `feat(eventstore): interface and value types`.

### Task 2: SQLite Open + Append with optimistic concurrency

**Files:** Create `eventstore/sqlite/sqlite.go`, `eventstore/sqlite/sqlite_test.go`.

- [ ] Write a failing test: open an in-memory store, `Append("s", NoStream, e1, e2)` returns a Position and no error; a second `Append("s", NoStream, e3)` returns a `*ConflictError{Expected: NoStream, Actual: 1}`; `Append("s", 1, e3)` succeeds. Run it, watch it fail to compile/pass.
- [ ] Implement `Open(dsn string) (*Store, error)` (creating the schema) and `Append` (the transactional read-max/check/insert above). Map a `UNIQUE` violation on `(stream, revision)` to `*ConflictError`. Run the test, watch it pass.
- [ ] Commit: `feat(eventstore/sqlite): Open and Append with optimistic concurrency`.

### Task 3: ReadStream + ReadAll (ordering)

**Files:** modify `eventstore/sqlite/sqlite.go`, `eventstore/sqlite/sqlite_test.go`.

- [ ] Failing test: after appending to streams "a" and "b" interleaved, `ReadStream("a", 0, Forward)` returns a's events in revision order with correct `Revision`s; `ReadAll(0, Forward)` returns ALL events in global `Position` order across both streams; `Backward` reverses. Run, watch fail.
- [ ] Implement `ReadStream` (`WHERE stream=? AND revision>=? ORDER BY revision`) and `ReadAll` (`WHERE position>=? ORDER BY position`), honoring `Direction`. Run, watch pass.
- [ ] Commit: `feat(eventstore/sqlite): ReadStream and ReadAll`.

### Task 4: SubscribeAll (catch-up: replay then live)

**Files:** modify `eventstore/sqlite/sqlite.go`, `eventstore/sqlite/sqlite_test.go`.

- [ ] Failing test: append e1,e2; `SubscribeAll(0)`; read e1,e2 off the channel (history); then `Append` e3 and assert e3 arrives on the channel within a timeout; `Close()` ends the channel. Run, watch fail.
- [ ] Implement `SubscribeAll`: a goroutine that first drains `position >= from`, then polls `position > lastSeen` every ~25ms, pushing onto a buffered channel; stop on `Close`/context. Run, watch pass (use a `select` with timeout in the test, not a sleep-assert).
- [ ] Commit: `feat(eventstore/sqlite): catch-up SubscribeAll`.

### Task 5: The shared contract suite

**Files:** Create `eventstore/eventstoretest/contract.go`; modify `eventstore/sqlite/sqlite_test.go`.

- [ ] Write `RunContract(t, factory)` consolidating the behaviors as table-style subtests: append+read ordering, global-order across streams, optimistic-concurrency (NoStream, exact, stale), catch-up replay-then-live. Each subtest builds a fresh store via `factory(t)`.
- [ ] In `sqlite_test.go`, add `TestSQLiteContract(t)` calling `RunContract(t, func(t) eventstore.EventStore { return mustOpenTemp(t) })`. Run `go test ./... -race`, watch all pass.
- [ ] Commit: `test(eventstore): backend-agnostic contract suite, sqlite passes it`.

## Done when

`go test ./... -race` is green, the contract suite passes against the SQLite backend, and `cmd/rigger` still builds (it is still empty, so this is a no-op until Phase 5). CI on `main` stays green.
