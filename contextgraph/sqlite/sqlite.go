// Package sqlite is the default context-graph projector: it folds the event log
// into bi-temporal node and edge tables in a local SQLite file (one per project)
// and answers Subgraph and Resolve. It is an adapter; it depends inward on the
// contextgraph port and the eventstore domain.
package sqlite

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	_ "modernc.org/sqlite" // registers the "sqlite" database/sql driver

	"github.com/virtual-velocitation/rigger/contextgraph"
	"github.com/virtual-velocitation/rigger/eventstore"
)

const schema = `
CREATE TABLE IF NOT EXISTS nodes (
  id    TEXT PRIMARY KEY,
  kind  TEXT NOT NULL,
  attrs TEXT
);
CREATE TABLE IF NOT EXISTS edges (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  from_id    TEXT NOT NULL,
  to_id      TEXT NOT NULL,
  rel        TEXT NOT NULL,
  valid_from TEXT NOT NULL,
  valid_to   TEXT,
  source     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to   ON edges(to_id);
CREATE TABLE IF NOT EXISTS aliases (
  alias        TEXT PRIMARY KEY,
  canonical_id TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS applied (
  position INTEGER PRIMARY KEY
);`

// Projector is the SQLite-backed contextgraph.Projection.
type Projector struct {
	db *sql.DB
}

var _ contextgraph.Projection = (*Projector)(nil)

// Open opens (creating if needed) the graph projection at path.
func Open(path string) (*Projector, error) {
	dsn := "file:" + path + "?_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)"
	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("graph: open: %w", err)
	}
	// Apply does a read-then-write transaction; serialize through one connection
	// so concurrent applies during a run queue instead of deadlocking (SQLITE_BUSY).
	db.SetMaxOpenConns(1)
	if _, err := db.Exec(schema); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("graph: schema: %w", err)
	}
	return &Projector{db: db}, nil
}

// Close releases the database.
func (p *Projector) Close() error { return p.db.Close() }

// Apply folds one event into the graph, exactly once per global position.
func (p *Projector) Apply(ctx context.Context, e eventstore.Event) error {
	tx, err := p.db.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("graph: begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	res, err := tx.ExecContext(ctx, `INSERT OR IGNORE INTO applied (position) VALUES (?)`, int64(e.Position))
	if err != nil {
		return fmt.Errorf("graph: mark applied: %w", err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return nil // already folded; replay is a no-op
	}

	if err := fold(ctx, tx, e); err != nil {
		return err
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("graph: commit: %w", err)
	}
	return nil
}

func fold(ctx context.Context, tx *sql.Tx, e eventstore.Event) error {
	at := e.RecordedAt.UTC().Format(time.RFC3339Nano)
	switch e.Type {
	case contextgraph.TypeDecisionMade:
		var d contextgraph.DecisionMade
		if err := json.Unmarshal(e.Data, &d); err != nil {
			return fmt.Errorf("graph: decode DecisionMade: %w", err)
		}
		if err := ensureNode(ctx, tx, d.ID, contextgraph.KindDecision, map[string]string{"summary": d.Summary}); err != nil {
			return err
		}
		for _, path := range d.Governs {
			if err := ensureNode(ctx, tx, path, contextgraph.KindArtifact, nil); err != nil {
				return err
			}
			if err := addEdge(ctx, tx, d.ID, path, contextgraph.RelGoverns, at, e.Position); err != nil {
				return err
			}
		}
		if d.Supersedes != "" {
			if err := ensureNode(ctx, tx, d.Supersedes, contextgraph.KindDecision, nil); err != nil {
				return err
			}
			if err := addEdge(ctx, tx, d.ID, d.Supersedes, contextgraph.RelSupersedes, at, e.Position); err != nil {
				return err
			}
			// Invalidate (do not delete) the superseded decision's governing edges.
			if _, err := tx.ExecContext(ctx,
				`UPDATE edges SET valid_to = ? WHERE from_id = ? AND rel = ? AND valid_to IS NULL`,
				at, d.Supersedes, contextgraph.RelGoverns); err != nil {
				return fmt.Errorf("graph: invalidate superseded: %w", err)
			}
		}
	case contextgraph.TypeFileTouched:
		var f contextgraph.FileTouched
		if err := json.Unmarshal(e.Data, &f); err != nil {
			return fmt.Errorf("graph: decode FileTouched: %w", err)
		}
		if err := ensureNode(ctx, tx, f.Path, contextgraph.KindArtifact, nil); err != nil {
			return err
		}
		if f.By != "" {
			if err := ensureNode(ctx, tx, f.By, contextgraph.KindAgent, nil); err != nil {
				return err
			}
			if err := addEdge(ctx, tx, f.By, f.Path, contextgraph.RelTouches, at, e.Position); err != nil {
				return err
			}
		}
	case contextgraph.TypeGateVerdict:
		var g contextgraph.GateVerdict
		if err := json.Unmarshal(e.Data, &g); err != nil {
			return fmt.Errorf("graph: decode GateVerdict: %w", err)
		}
		if err := ensureNode(ctx, tx, g.Gate, contextgraph.KindGate, map[string]string{"pass": fmt.Sprintf("%t", g.Pass)}); err != nil {
			return err
		}
		if g.Artifact != "" {
			if err := ensureNode(ctx, tx, g.Artifact, contextgraph.KindArtifact, nil); err != nil {
				return err
			}
			if err := addEdge(ctx, tx, g.Artifact, g.Gate, contextgraph.RelGatedBy, at, e.Position); err != nil {
				return err
			}
		}
	case contextgraph.TypeUnitIntegrated:
		var u contextgraph.UnitIntegrated
		if err := json.Unmarshal(e.Data, &u); err != nil {
			return fmt.Errorf("graph: decode UnitIntegrated: %w", err)
		}
		if err := ensureNode(ctx, tx, u.Unit, contextgraph.KindUnit, map[string]string{"commit": u.Commit, "status": "integrated"}); err != nil {
			return err
		}
	default:
		// An event type the graph does not model is simply ignored.
	}
	return nil
}

func ensureNode(ctx context.Context, tx *sql.Tx, id, kind string, attrs map[string]string) error {
	var attrJSON any
	if len(attrs) > 0 {
		b, err := json.Marshal(attrs)
		if err != nil {
			return fmt.Errorf("graph: encode attrs: %w", err)
		}
		attrJSON = string(b)
	}
	_, err := tx.ExecContext(ctx,
		`INSERT INTO nodes (id, kind, attrs) VALUES (?, ?, ?)
		 ON CONFLICT(id) DO UPDATE SET attrs = COALESCE(excluded.attrs, nodes.attrs)`,
		id, kind, attrJSON)
	if err != nil {
		return fmt.Errorf("graph: ensure node %q: %w", id, err)
	}
	return nil
}

func addEdge(ctx context.Context, tx *sql.Tx, from, to, rel, at string, src eventstore.Position) error {
	_, err := tx.ExecContext(ctx,
		`INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source) VALUES (?, ?, ?, ?, NULL, ?)`,
		from, to, rel, at, int64(src))
	if err != nil {
		return fmt.Errorf("graph: add edge %s-%s->%s: %w", from, rel, to, err)
	}
	return nil
}

// Subgraph returns the connected subgraph reachable from any seed within depth
// hops, following only currently valid edges.
func (p *Projector) Subgraph(ctx context.Context, seed []string, depth int) (contextgraph.Graph, error) {
	seedJSON, err := json.Marshal(seed)
	if err != nil {
		return contextgraph.Graph{}, fmt.Errorf("graph: encode seed: %w", err)
	}
	ids, err := p.reachable(ctx, string(seedJSON), depth)
	if err != nil {
		return contextgraph.Graph{}, err
	}
	if len(ids) == 0 {
		return contextgraph.Graph{}, nil
	}
	nodes, err := p.nodesByID(ctx, ids)
	if err != nil {
		return contextgraph.Graph{}, err
	}
	edges, err := p.validEdgesAmong(ctx, ids)
	if err != nil {
		return contextgraph.Graph{}, err
	}
	return contextgraph.Graph{Nodes: nodes, Edges: edges}, nil
}

func (p *Projector) reachable(ctx context.Context, seedJSON string, depth int) ([]string, error) {
	rows, err := p.db.QueryContext(ctx, `
WITH RECURSIVE reach(id, depth) AS (
  SELECT value, 0 FROM json_each(?)
  UNION
  SELECT CASE WHEN e.from_id = r.id THEN e.to_id ELSE e.from_id END, r.depth + 1
  FROM reach r
  JOIN edges e ON (e.from_id = r.id OR e.to_id = r.id) AND e.valid_to IS NULL
  WHERE r.depth < ?
)
SELECT DISTINCT id FROM reach`, seedJSON, depth)
	if err != nil {
		return nil, fmt.Errorf("graph: reach: %w", err)
	}
	defer func() { _ = rows.Close() }()
	var ids []string
	for rows.Next() {
		var id string
		if err := rows.Scan(&id); err != nil {
			return nil, fmt.Errorf("graph: scan reach: %w", err)
		}
		ids = append(ids, id)
	}
	return ids, rows.Err()
}

func (p *Projector) nodesByID(ctx context.Context, ids []string) ([]contextgraph.Node, error) {
	ph, args := placeholders(ids)
	rows, err := p.db.QueryContext(ctx, `SELECT id, kind, attrs FROM nodes WHERE id IN (`+ph+`)`, args...)
	if err != nil {
		return nil, fmt.Errorf("graph: nodes: %w", err)
	}
	defer func() { _ = rows.Close() }()
	var out []contextgraph.Node
	for rows.Next() {
		var n contextgraph.Node
		var attrs sql.NullString
		if err := rows.Scan(&n.ID, &n.Kind, &attrs); err != nil {
			return nil, fmt.Errorf("graph: scan node: %w", err)
		}
		if attrs.Valid && attrs.String != "" {
			if err := json.Unmarshal([]byte(attrs.String), &n.Attrs); err != nil {
				return nil, fmt.Errorf("graph: decode attrs: %w", err)
			}
		}
		out = append(out, n)
	}
	return out, rows.Err()
}

func (p *Projector) validEdgesAmong(ctx context.Context, ids []string) ([]contextgraph.Edge, error) {
	ph, args := placeholders(ids)
	q := `SELECT from_id, to_id, rel, valid_from, source FROM edges
	      WHERE valid_to IS NULL AND from_id IN (` + ph + `) AND to_id IN (` + ph + `)`
	rows, err := p.db.QueryContext(ctx, q, append(args, args...)...)
	if err != nil {
		return nil, fmt.Errorf("graph: edges: %w", err)
	}
	defer func() { _ = rows.Close() }()
	var out []contextgraph.Edge
	for rows.Next() {
		var (
			e         contextgraph.Edge
			validFrom string
			src       int64
		)
		if err := rows.Scan(&e.From, &e.To, &e.Rel, &validFrom, &src); err != nil {
			return nil, fmt.Errorf("graph: scan edge: %w", err)
		}
		if e.ValidFrom, err = time.Parse(time.RFC3339Nano, validFrom); err != nil {
			return nil, fmt.Errorf("graph: parse valid_from: %w", err)
		}
		e.Source = eventstore.Position(src)
		out = append(out, e)
	}
	return out, rows.Err()
}

// Resolve maps a mention to a canonical node id via the alias table, falling
// back to a direct node-id match.
func (p *Projector) Resolve(ctx context.Context, mention string) (string, bool, error) {
	var canonical string
	err := p.db.QueryRowContext(ctx, `SELECT canonical_id FROM aliases WHERE alias = ?`, mention).Scan(&canonical)
	switch {
	case err == nil:
		return canonical, true, nil
	case !errors.Is(err, sql.ErrNoRows):
		return "", false, fmt.Errorf("graph: resolve alias: %w", err)
	}
	var id string
	err = p.db.QueryRowContext(ctx, `SELECT id FROM nodes WHERE id = ?`, mention).Scan(&id)
	switch {
	case err == nil:
		return id, true, nil
	case errors.Is(err, sql.ErrNoRows):
		return "", false, nil
	default:
		return "", false, fmt.Errorf("graph: resolve node: %w", err)
	}
}

func placeholders(ids []string) (string, []any) {
	marks := make([]string, len(ids))
	args := make([]any, len(ids))
	for i, id := range ids {
		marks[i] = "?"
		args[i] = id
	}
	return strings.Join(marks, ", "), args
}
