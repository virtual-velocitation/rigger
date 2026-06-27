// Package sqlite is the default, embedded EventStore adapter. It is pure Go
// (modernc.org/sqlite, no cgo), stores the whole log in one file, and is the
// reference implementation the backend-agnostic contract suite runs against.
//
// It is an adapter in the Clean-Architecture sense: it depends inward on the
// eventstore port and is never depended upon by the core.
package sqlite

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	_ "modernc.org/sqlite" // registers the "sqlite" database/sql driver

	"github.com/virtual-velocitation/rigger/eventstore"
)

// noStream is the revision of an absent stream: MAX(revision) coalesced to -1.
const noStream = eventstore.Revision(-1)

const schema = `
CREATE TABLE IF NOT EXISTS events (
  position    INTEGER PRIMARY KEY AUTOINCREMENT,
  stream      TEXT    NOT NULL,
  revision    INTEGER NOT NULL,
  id          TEXT    NOT NULL,
  type        TEXT    NOT NULL,
  data        BLOB,
  meta        TEXT,
  valid_from  TEXT    NOT NULL,
  recorded_at TEXT    NOT NULL,
  UNIQUE(stream, revision),
  UNIQUE(stream, id)
);
CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream, revision);`

// Store is the SQLite-backed eventstore.EventStore.
type Store struct {
	db *sql.DB
}

var _ eventstore.EventStore = (*Store)(nil)

// Open opens (creating if needed) the event store at path and ensures its
// schema. WAL mode plus a busy timeout let the subscription poller read while
// appends commit.
func Open(path string) (*Store, error) {
	dsn := "file:" + path + "?_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)"
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("sqlite: open: %w", err)
	}
	// Append does a read-then-write in one transaction; two connections each
	// holding a read lock and trying to upgrade to a write lock deadlock with
	// SQLITE_BUSY. Serializing through one connection makes concurrent appends
	// queue cleanly instead. (Reads are quick, so this is not a bottleneck at
	// the harness's event volume.)
	db.SetMaxOpenConns(1)
	if err := db.PingContext(context.Background()); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("sqlite: ping: %w", err)
	}
	if _, err := db.Exec(schema); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("sqlite: schema: %w", err)
	}
	return &Store{db: db}, nil
}

// Close releases the underlying database.
func (s *Store) Close() error { return s.db.Close() }

// Append writes events to the end of a stream under the optimistic expectation.
func (s *Store) Append(ctx context.Context, stream string, expected eventstore.ExpectedRevision, events ...eventstore.Event) (eventstore.Position, error) {
	if len(events) == 0 {
		return 0, nil
	}
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return 0, fmt.Errorf("sqlite: begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	var lastInt int64
	if err := tx.QueryRowContext(ctx,
		`SELECT COALESCE(MAX(revision), -1) FROM events WHERE stream = ?`, stream).Scan(&lastInt); err != nil {
		return 0, fmt.Errorf("sqlite: read revision: %w", err)
	}
	last := eventstore.Revision(lastInt)

	if cerr := checkExpected(stream, expected, last); cerr != nil {
		return 0, cerr
	}

	now := time.Now().UTC()
	var lastPos eventstore.Position
	for i, e := range events {
		rev := last + 1 + eventstore.Revision(i)
		meta, err := encodeMeta(e.Meta)
		if err != nil {
			return 0, fmt.Errorf("sqlite: encode meta: %w", err)
		}
		validFrom := e.ValidFrom
		if validFrom.IsZero() {
			validFrom = now
		}
		res, err := tx.ExecContext(ctx,
			`INSERT INTO events (stream, revision, id, type, data, meta, valid_from, recorded_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
			stream, int64(rev), e.ID, e.Type, e.Data, meta,
			validFrom.Format(time.RFC3339Nano), now.Format(time.RFC3339Nano))
		if err != nil {
			// A concurrent appender that won the race trips UNIQUE(stream,
			// revision); report it as a conflict, not a raw driver error.
			if isUniqueViolation(err) {
				return 0, &eventstore.ConflictError{Stream: stream, Expected: expected, Actual: last}
			}
			return 0, fmt.Errorf("sqlite: insert: %w", err)
		}
		pos, err := res.LastInsertId()
		if err != nil {
			return 0, fmt.Errorf("sqlite: last insert id: %w", err)
		}
		lastPos = eventstore.Position(pos)
	}
	if err := tx.Commit(); err != nil {
		return 0, fmt.Errorf("sqlite: commit: %w", err)
	}
	return lastPos, nil
}

// ReadStream returns one stream's events from a revision, in a direction.
func (s *Store) ReadStream(ctx context.Context, stream string, from eventstore.Revision, dir eventstore.Direction) ([]eventstore.Event, error) {
	cmp, order := "revision >= ?", "ASC"
	if dir == eventstore.Backward {
		cmp, order = "revision <= ?", "DESC"
	}
	q := fmt.Sprintf(`%s WHERE stream = ? AND %s ORDER BY revision %s`, selectColumns, cmp, order)
	rows, err := s.db.QueryContext(ctx, q, stream, int64(from))
	if err != nil {
		return nil, fmt.Errorf("sqlite: read stream: %w", err)
	}
	defer func() { _ = rows.Close() }()
	return scanEvents(rows)
}

// ReadAll returns events across all streams from a global position, in global
// order, narrowed by filter.
func (s *Store) ReadAll(ctx context.Context, from eventstore.Position, dir eventstore.Direction, filter eventstore.Filter) ([]eventstore.Event, error) {
	cmp, order := "position >= ?", "ASC"
	if dir == eventstore.Backward {
		cmp, order = "position <= ?", "DESC"
	}
	where := cmp
	args := []any{int64(from)}
	if filter.StreamPrefix != "" {
		// instr(stream, prefix) = 1 means the stream begins with prefix, with
		// no LIKE/GLOB wildcard hazards from the prefix itself.
		where += " AND instr(stream, ?) = 1"
		args = append(args, filter.StreamPrefix)
	}
	q := fmt.Sprintf(`%s WHERE %s ORDER BY position %s`, selectColumns, where, order)
	rows, err := s.db.QueryContext(ctx, q, args...)
	if err != nil {
		return nil, fmt.Errorf("sqlite: read all: %w", err)
	}
	defer func() { _ = rows.Close() }()
	return scanEvents(rows)
}

// SubscribeAll returns a catch-up subscription: history from `from`, then live
// events, narrowed by filter. The poller advances a cursor over the global
// position and never re-reads delivered events.
func (s *Store) SubscribeAll(ctx context.Context, from eventstore.Position, filter eventstore.Filter) (eventstore.Subscription, error) {
	subCtx, cancel := context.WithCancel(ctx)
	sub := &subscription{events: make(chan eventstore.Event, 128), cancel: cancel}
	go sub.run(subCtx, s, from, filter)
	return sub, nil
}

const selectColumns = `SELECT position, stream, revision, id, type, data, meta, valid_from, recorded_at FROM events`

func scanEvents(rows *sql.Rows) ([]eventstore.Event, error) {
	var out []eventstore.Event
	for rows.Next() {
		var (
			e               eventstore.Event
			pos, rev        int64
			meta            sql.NullString
			validF, recordA string
		)
		if err := rows.Scan(&pos, &e.Stream, &rev, &e.ID, &e.Type, &e.Data, &meta, &validF, &recordA); err != nil {
			return nil, fmt.Errorf("sqlite: scan: %w", err)
		}
		e.Position = eventstore.Position(pos)
		e.Revision = eventstore.Revision(rev)
		if meta.Valid && meta.String != "" {
			if err := json.Unmarshal([]byte(meta.String), &e.Meta); err != nil {
				return nil, fmt.Errorf("sqlite: decode meta: %w", err)
			}
		}
		var err error
		if e.ValidFrom, err = time.Parse(time.RFC3339Nano, validF); err != nil {
			return nil, fmt.Errorf("sqlite: parse valid_from: %w", err)
		}
		if e.RecordedAt, err = time.Parse(time.RFC3339Nano, recordA); err != nil {
			return nil, fmt.Errorf("sqlite: parse recorded_at: %w", err)
		}
		out = append(out, e)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("sqlite: rows: %w", err)
	}
	return out, nil
}

func checkExpected(stream string, expected eventstore.ExpectedRevision, actual eventstore.Revision) error {
	switch expected {
	case eventstore.Any:
		return nil
	case eventstore.NoStream:
		if actual != noStream {
			return &eventstore.ConflictError{Stream: stream, Expected: expected, Actual: actual}
		}
	default:
		if eventstore.Revision(expected) != actual {
			return &eventstore.ConflictError{Stream: stream, Expected: expected, Actual: actual}
		}
	}
	return nil
}

func encodeMeta(m map[string]string) (sql.NullString, error) {
	if len(m) == 0 {
		return sql.NullString{}, nil
	}
	b, err := json.Marshal(m)
	if err != nil {
		return sql.NullString{}, err
	}
	return sql.NullString{String: string(b), Valid: true}, nil
}

func isUniqueViolation(err error) bool {
	return err != nil && strings.Contains(err.Error(), "UNIQUE constraint failed")
}

// subscription is the catch-up subscription returned by SubscribeAll.
type subscription struct {
	events    chan eventstore.Event
	cancel    context.CancelFunc
	closeOnce sync.Once

	mu  sync.Mutex
	err error
}

func (sub *subscription) Events() <-chan eventstore.Event { return sub.events }

func (sub *subscription) Err() error {
	sub.mu.Lock()
	defer sub.mu.Unlock()
	return sub.err
}

func (sub *subscription) Close() error {
	sub.closeOnce.Do(sub.cancel)
	return nil
}

func (sub *subscription) setErr(err error) {
	if err == nil || errors.Is(err, context.Canceled) {
		return // a deliberate Close/cancel is not a terminal error
	}
	sub.mu.Lock()
	sub.err = err
	sub.mu.Unlock()
}

func (sub *subscription) run(ctx context.Context, s *Store, from eventstore.Position, filter eventstore.Filter) {
	defer close(sub.events)
	const interval = 25 * time.Millisecond
	next := from
	timer := time.NewTimer(0)
	defer timer.Stop()
	for {
		select {
		case <-ctx.Done():
			sub.setErr(ctx.Err())
			return
		case <-timer.C:
		}
		evs, err := s.ReadAll(ctx, next, eventstore.Forward, filter)
		if err != nil {
			sub.setErr(err)
			return
		}
		for _, e := range evs {
			select {
			case sub.events <- e:
				next = e.Position + 1
			case <-ctx.Done():
				sub.setErr(ctx.Err())
				return
			}
		}
		timer.Reset(interval)
	}
}
