//! The default context-graph projector: it folds the event log into bi-temporal
//! node and edge tables in a local SQLite file and answers Subgraph and Resolve.
//! A single connection behind a mutex serializes the read-then-write of apply.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::{
    Edge, Error, Graph, Node, Projection, KIND_AGENT, KIND_ARTIFACT, KIND_DECISION, KIND_GATE,
    KIND_LESSON, KIND_UNIT, REL_ABOUT, REL_GATED_BY, REL_GOVERNS, REL_SUPERSEDES, REL_TOUCHES,
    TYPE_DECISION_MADE, TYPE_FILE_TOUCHED, TYPE_GATE_VERDICT, TYPE_LESSON_LEARNED,
    TYPE_UNIT_INTEGRATED,
};
use crate::eventstore::{Event, Position};

const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
CREATE TABLE IF NOT EXISTS nodes (id TEXT PRIMARY KEY, kind TEXT NOT NULL, attrs TEXT);
CREATE TABLE IF NOT EXISTS edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
  valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
CREATE TABLE IF NOT EXISTS aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS applied (position INTEGER PRIMARY KEY);
";

/// Projector is the SQLite-backed Projection.
pub struct Projector {
    conn: Mutex<Connection>,
}

impl Projector {
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = Connection::open(path).map_err(be)?;
        conn.execute_batch(SCHEMA).map_err(be)?;
        Ok(Projector {
            conn: Mutex::new(conn),
        })
    }
}

fn be<E: std::fmt::Display>(e: E) -> Error {
    Error(e.to_string())
}

fn to_nanos(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

impl Projection for Projector {
    fn apply(&self, e: &Event) -> Result<(), Error> {
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction().map_err(be)?;
        let inserted = tx
            .execute(
                "INSERT OR IGNORE INTO applied (position) VALUES (?1)",
                [e.position as i64],
            )
            .map_err(be)?;
        if inserted > 0 {
            fold(&tx, e)?;
        }
        tx.commit().map_err(be)?;
        Ok(())
    }

    fn subgraph(&self, seed: &[String], depth: i64) -> Result<Graph, Error> {
        let seed_json = serde_json::to_string(seed).map_err(be)?;
        let conn = self.conn.lock().unwrap();

        let mut reach = conn
            .prepare(
                "WITH RECURSIVE reach(id, depth) AS (
                   SELECT value, 0 FROM json_each(?1)
                   UNION
                   SELECT CASE WHEN e.from_id = r.id THEN e.to_id ELSE e.from_id END, r.depth + 1
                   FROM reach r JOIN edges e
                     ON (e.from_id = r.id OR e.to_id = r.id) AND e.valid_to IS NULL
                   WHERE r.depth < ?2
                 )
                 SELECT DISTINCT id FROM reach",
            )
            .map_err(be)?;
        let ids: Vec<String> = reach
            .query_map(params![seed_json, depth], |r| r.get(0))
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;
        if ids.is_empty() {
            return Ok(Graph::default());
        }
        let ids_json = serde_json::to_string(&ids).map_err(be)?;

        let mut nstmt = conn
            .prepare(
                "SELECT id, kind, attrs FROM nodes WHERE id IN (SELECT value FROM json_each(?1))",
            )
            .map_err(be)?;
        let nodes: Vec<Node> = nstmt
            .query_map([&ids_json], row_to_node)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;

        let mut estmt = conn
            .prepare(
                "SELECT from_id, to_id, rel, valid_from, source FROM edges
                 WHERE valid_to IS NULL
                   AND from_id IN (SELECT value FROM json_each(?1))
                   AND to_id IN (SELECT value FROM json_each(?1))",
            )
            .map_err(be)?;
        let edges: Vec<Edge> = estmt
            .query_map([&ids_json], row_to_edge)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;

        Ok(Graph { nodes, edges })
    }

    fn resolve(&self, mention: &str) -> Result<Option<String>, Error> {
        let conn = self.conn.lock().unwrap();
        let canonical: Option<String> = conn
            .query_row(
                "SELECT canonical_id FROM aliases WHERE alias = ?1",
                [mention],
                |r| r.get(0),
            )
            .optional()
            .map_err(be)?;
        if canonical.is_some() {
            return Ok(canonical);
        }
        conn.query_row("SELECT id FROM nodes WHERE id = ?1", [mention], |r| {
            r.get(0)
        })
        .optional()
        .map_err(be)
    }
}

fn row_to_node(r: &rusqlite::Row) -> rusqlite::Result<Node> {
    let attrs_str: Option<String> = r.get(2)?;
    let attrs = attrs_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    Ok(Node {
        id: r.get(0)?,
        kind: r.get(1)?,
        attrs,
    })
}

fn row_to_edge(r: &rusqlite::Row) -> rusqlite::Result<Edge> {
    Ok(Edge {
        from: r.get(0)?,
        to: r.get(1)?,
        rel: r.get(2)?,
        valid_from: r.get(3)?,
        valid_to: None,
        source: r.get::<_, i64>(4)? as Position,
    })
}

fn fold(tx: &Transaction, e: &Event) -> Result<(), Error> {
    let at = to_nanos(e.recorded_at);
    match e.type_.as_str() {
        TYPE_DECISION_MADE => {
            let d: super::DecisionMade = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &d.id, KIND_DECISION, &[("summary", &d.summary)])?;
            for path in &d.governs {
                ensure_node(tx, path, KIND_ARTIFACT, &[])?;
                add_edge(tx, &d.id, path, REL_GOVERNS, at, e.position)?;
            }
            if !d.supersedes.is_empty() {
                ensure_node(tx, &d.supersedes, KIND_DECISION, &[])?;
                add_edge(tx, &d.id, &d.supersedes, REL_SUPERSEDES, at, e.position)?;
                // Invalidate (never delete) the superseded decision's governing edges.
                tx.execute(
                    "UPDATE edges SET valid_to = ?1 WHERE from_id = ?2 AND rel = ?3 AND valid_to IS NULL",
                    params![at, d.supersedes, REL_GOVERNS],
                )
                .map_err(be)?;
            }
        }
        TYPE_FILE_TOUCHED => {
            let f: super::FileTouched = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &f.path, KIND_ARTIFACT, &[])?;
            if !f.by.is_empty() {
                ensure_node(tx, &f.by, KIND_AGENT, &[])?;
                add_edge(tx, &f.by, &f.path, REL_TOUCHES, at, e.position)?;
            }
        }
        TYPE_GATE_VERDICT => {
            let g: super::GateVerdict = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &g.gate, KIND_GATE, &[("pass", &g.pass.to_string())])?;
            if !g.artifact.is_empty() {
                ensure_node(tx, &g.artifact, KIND_ARTIFACT, &[])?;
                add_edge(tx, &g.artifact, &g.gate, REL_GATED_BY, at, e.position)?;
            }
        }
        TYPE_UNIT_INTEGRATED => {
            let u: super::UnitIntegrated = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &u.unit,
                KIND_UNIT,
                &[("commit", &u.commit), ("status", "integrated")],
            )?;
        }
        TYPE_LESSON_LEARNED => {
            let l: super::LessonLearned = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &l.id, KIND_LESSON, &[("summary", &l.summary)])?;
            for path in &l.about {
                ensure_node(tx, path, KIND_ARTIFACT, &[])?;
                add_edge(tx, &l.id, path, REL_ABOUT, at, e.position)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn ensure_node(
    tx: &Transaction,
    id: &str,
    kind: &str,
    attrs: &[(&str, &str)],
) -> Result<(), Error> {
    let attr_json: Option<String> = if attrs.is_empty() {
        None
    } else {
        let map: BTreeMap<&str, &str> = attrs.iter().copied().collect();
        Some(serde_json::to_string(&map).map_err(be)?)
    };
    tx.execute(
        "INSERT INTO nodes (id, kind, attrs) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET attrs = COALESCE(excluded.attrs, nodes.attrs)",
        params![id, kind, attr_json],
    )
    .map_err(be)?;
    Ok(())
}

fn add_edge(
    tx: &Transaction,
    from: &str,
    to: &str,
    rel: &str,
    at: i64,
    src: Position,
) -> Result<(), Error> {
    tx.execute(
        "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
        params![from, to, rel, at, src as i64],
    )
    .map_err(be)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_decision(
        p: &Projector,
        pos: u64,
        id: &str,
        summary: &str,
        governs: &[&str],
        supersedes: &str,
    ) {
        let payload = serde_json::json!({
            "id": id, "summary": summary, "governs": governs, "supersedes": supersedes,
        });
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = pos;
        p.apply(&e).unwrap();
    }

    #[test]
    fn subgraph_finds_the_governing_decision() {
        let p = Projector::open(":memory:").unwrap();
        apply_decision(&p, 1, "d1", "uses the generic pipeline", &["mod.rs"], "");
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let d = g
            .nodes
            .iter()
            .find(|n| n.id == "d1")
            .expect("d1 reachable from mod.rs");
        assert_eq!(
            d.attrs.get("summary").map(String::as_str),
            Some("uses the generic pipeline")
        );
    }

    #[test]
    fn supersession_invalidates_the_old_governing_edge() {
        let p = Projector::open(":memory:").unwrap();
        apply_decision(&p, 1, "d1", "old", &["mod.rs"], "");
        apply_decision(&p, 2, "d2", "new", &["mod.rs"], "d1");
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let governs: Vec<(&str, &str)> = g
            .edges
            .iter()
            .filter(|e| e.rel == REL_GOVERNS)
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .collect();
        assert!(
            governs.contains(&("d2", "mod.rs")),
            "d2 currently governs mod.rs"
        );
        assert!(
            !governs.contains(&("d1", "mod.rs")),
            "d1's GOVERNS edge was invalidated"
        );
    }

    #[test]
    fn apply_is_idempotent_per_position() {
        let p = Projector::open(":memory:").unwrap();
        apply_decision(&p, 1, "d1", "x", &["mod.rs"], "");
        apply_decision(&p, 1, "d1", "x", &["mod.rs"], ""); // same position, replayed
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let governs = g.edges.iter().filter(|e| e.rel == REL_GOVERNS).count();
        assert_eq!(governs, 1, "a replayed event must not double the edge");
    }
}
