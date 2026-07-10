//! The default context-graph projector: it folds the event log into bi-temporal
//! node and edge tables in a local SQLite file and answers Subgraph and Resolve.
//! A single connection behind a mutex serializes the read-then-write of apply.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::{
    Edge, Error, Graph, Node, Projection, KIND_AGENT, KIND_ARTIFACT, KIND_DECISION, KIND_FINDING,
    KIND_GATE, KIND_LESSON, KIND_UNIT, META_ACTOR, REL_ABOUT, REL_ASSIGNED_TO, REL_BLOCKS,
    REL_DECIDED, REL_GATED_BY, REL_GOVERNS, REL_RAISED, REL_SUPERSEDES, REL_TOUCHES,
    TYPE_ALIAS_DEFINED, TYPE_ALIAS_UNRESOLVED, TYPE_DECISION_MADE, TYPE_FILE_TOUCHED,
    TYPE_GATE_VERDICT, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING, TYPE_UNIT_INTEGRATED,
    TYPE_UNIT_STARTED,
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
    // The edge's bi-temporal valid-time is when the fact became true (the event's
    // caller-supplied valid_from), not the ingest time.
    let at = to_nanos(e.valid_from);
    match e.type_.as_str() {
        TYPE_DECISION_MADE => {
            let d: super::DecisionMade = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &d.id, KIND_DECISION, &[("summary", &d.summary)])?;
            // DECIDED: the acting agent (from event metadata) made this decision.
            if let Some(actor) = e.meta.get(META_ACTOR).filter(|a| !a.is_empty()) {
                ensure_node(tx, actor, KIND_AGENT, &[])?;
                add_edge(tx, actor, &d.id, REL_DECIDED, at, e.position)?;
            }
            for path in &d.governs {
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[])?;
                add_edge(tx, &d.id, &canonical, REL_GOVERNS, at, e.position)?;
            }
            if !d.supersedes.is_empty() {
                ensure_node(tx, &d.supersedes, KIND_DECISION, &[])?;
                add_edge(tx, &d.id, &d.supersedes, REL_SUPERSEDES, at, e.position)?;
                // Invalidate (never delete) the governing edges the superseded
                // decision asserted.
                tx.execute(
                    "UPDATE edges SET valid_to = ?1 WHERE from_id = ?2 AND rel = ?3 AND valid_to IS NULL",
                    params![at, d.supersedes, REL_GOVERNS],
                )
                .map_err(be)?;
            }
        }
        TYPE_FILE_TOUCHED => {
            let f: super::FileTouched = serde_json::from_slice(&e.data).map_err(be)?;
            let path = resolve_in_tx(tx, &f.path);
            ensure_node(tx, &path, KIND_ARTIFACT, &[])?;
            if !f.by.is_empty() {
                ensure_node(tx, &f.by, KIND_AGENT, &[])?;
                add_edge(tx, &f.by, &path, REL_TOUCHES, at, e.position)?;
            }
        }
        TYPE_GATE_VERDICT => {
            let g: super::GateVerdict = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &g.gate, KIND_GATE, &[("pass", &g.pass.to_string())])?;
            if !g.artifact.is_empty() {
                let artifact = resolve_in_tx(tx, &g.artifact);
                ensure_node(tx, &artifact, KIND_ARTIFACT, &[])?;
                add_edge(tx, &artifact, &g.gate, REL_GATED_BY, at, e.position)?;
            }
        }
        TYPE_UNIT_STARTED => {
            let u: super::UnitStarted = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &u.unit,
                KIND_UNIT,
                &[("criterion", &u.criterion), ("status", "started")],
            )?;
            // ASSIGNED_TO: the unit is assigned to its agent.
            if !u.agent.is_empty() {
                ensure_node(tx, &u.agent, KIND_AGENT, &[])?;
                add_edge(tx, &u.unit, &u.agent, REL_ASSIGNED_TO, at, e.position)?;
            }
            // BLOCKS: each dependency blocks this unit until it lands.
            for need in &u.needs {
                ensure_node(tx, need, KIND_UNIT, &[])?;
                add_edge(tx, need, &u.unit, REL_BLOCKS, at, e.position)?;
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
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[])?;
                add_edge(tx, &l.id, &canonical, REL_ABOUT, at, e.position)?;
            }
        }
        TYPE_REVIEW_FINDING => {
            // A review finding the lenses / adversary raise about a unit's files:
            // the cross-agent memory the three tiers communicate THROUGH. The finding
            // node carries the summary, the reviewer (`by`), and the unit; an ABOUT
            // edge ties it to each file it concerns (so a later reviewer grounded on
            // those files reaches it the same way it reaches the decisions that GOVERN
            // them); and a RAISED edge records the reviewer's provenance (the
            // DECIDED-style link). The actor metadata, when present, takes precedence
            // over `by` as the provenance source so it matches the other folds.
            let f: super::ReviewFinding = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &f.id,
                KIND_FINDING,
                &[("summary", &f.summary), ("by", &f.by), ("unit", &f.unit)],
            )?;
            let raiser = e
                .meta
                .get(META_ACTOR)
                .filter(|a| !a.is_empty())
                .map(String::as_str)
                .unwrap_or(f.by.as_str());
            if !raiser.is_empty() {
                ensure_node(tx, raiser, KIND_AGENT, &[])?;
                add_edge(tx, raiser, &f.id, REL_RAISED, at, e.position)?;
            }
            for path in &f.about {
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[])?;
                add_edge(tx, &f.id, &canonical, REL_ABOUT, at, e.position)?;
            }
        }
        TYPE_ALIAS_DEFINED => {
            let a: super::AliasDefined = serde_json::from_slice(&e.data).map_err(be)?;
            tx.execute(
                "INSERT INTO aliases (alias, canonical_id) VALUES (?1, ?2)
                 ON CONFLICT(alias) DO UPDATE SET canonical_id = excluded.canonical_id",
                params![a.alias, a.canonical],
            )
            .map_err(be)?;
        }
        TYPE_ALIAS_UNRESOLVED => {
            let a: super::AliasUnresolved = serde_json::from_slice(&e.data).map_err(be)?;
            // Create the node and mark it unresolved for later merge (never drop).
            ensure_node(tx, &a.mention, KIND_ARTIFACT, &[("unresolved", "true")])?;
        }
        _ => {}
    }
    Ok(())
}

/// Collapse a mention onto its canonical node via the alias table; an unknown
/// mention resolves to itself.
fn resolve_in_tx(tx: &Transaction, mention: &str) -> String {
    tx.query_row(
        "SELECT canonical_id FROM aliases WHERE alias = ?1",
        [mention],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or_else(|| mention.to_string())
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

    #[test]
    fn a_blast_radius_computed_event_folds_to_nothing_idempotently() {
        // spec 16 unit 3: BlastRadiusComputed is PURE AUDIT - the projector matches no fold arm
        // for it (it falls to the `_ => {}` sink), so it adds NO node and NO edge, and re-applying
        // the SAME position (a replay) stays a no-op. This is what lets the audit ride the shared
        // stream without perturbing the context graph the reviewers read.
        let p = Projector::open(":memory:").unwrap();
        let payload = serde_json::json!({
            "id": "u1",
            "unit": "u1",
            "precise": ["a.rs"],
            "safe": ["a.rs", "b.rs"],
            "serialize": false,
            "index_stamp": "h/v",
        });
        let mut e = Event::new(
            crate::conductor::TYPE_BLAST_RADIUS_COMPUTED,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = 1;
        p.apply(&e).unwrap();
        p.apply(&e).unwrap(); // same position, replayed: still a no-op
        for seed in [["u1"], ["a.rs"], ["b.rs"]] {
            let g = p
                .subgraph(&seed.iter().map(|s| s.to_string()).collect::<Vec<_>>(), 2)
                .unwrap();
            assert!(
                g.nodes.is_empty() && g.edges.is_empty(),
                "a BlastRadiusComputed event folds to no node/edge; got {g:?}"
            );
        }
    }

    #[test]
    fn decided_edge_links_the_acting_agent() {
        let p = Projector::open(":memory:").unwrap();
        let payload = serde_json::json!({"id": "d1", "summary": "x", "governs": ["mod.rs"]});
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        e.meta.insert(META_ACTOR.to_string(), "agent-7".to_string());
        p.apply(&e).unwrap();
        let g = p.subgraph(&["d1".to_string()], 2).unwrap();
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_DECIDED && x.from == "agent-7" && x.to == "d1"),
            "DECIDED(agent-7 -> d1) must come from the event actor"
        );
    }

    #[test]
    fn review_finding_creates_a_finding_node_about_each_file() {
        // A ReviewFinding folds into a KIND_FINDING node carrying its summary, an
        // ABOUT edge to each file it concerns, and a RAISED edge from the reviewer.
        // The finding is reachable from the file it is ABOUT - the same traversal that
        // returns the decisions GOVERNING the file - so a later reviewer grounded on
        // that file retrieves it through the graph, not via hand-threaded prompts.
        let p = Projector::open(":memory:").unwrap();
        let payload = serde_json::json!({
            "id": "f1",
            "by": "tech-lens",
            "unit": "u1",
            "summary": "the new path skips the buffer authority",
            "about": ["combat.rs"],
        });
        let mut e = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        p.apply(&e).unwrap();

        // Reachable from the file it is ABOUT.
        let g = p.subgraph(&["combat.rs".to_string()], 2).unwrap();
        let n = g
            .nodes
            .iter()
            .find(|n| n.id == "f1")
            .expect("the finding node is reachable from the file it is ABOUT");
        assert_eq!(n.kind, KIND_FINDING);
        assert_eq!(
            n.attrs.get("summary").map(String::as_str),
            Some("the new path skips the buffer authority")
        );
        assert_eq!(n.attrs.get("by").map(String::as_str), Some("tech-lens"));
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_ABOUT && x.from == "f1" && x.to == "combat.rs"),
            "ABOUT(f1 -> combat.rs)"
        );
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_RAISED && x.from == "tech-lens" && x.to == "f1"),
            "RAISED(tech-lens -> f1): the reviewer's provenance"
        );
    }

    #[test]
    fn review_finding_actor_meta_takes_precedence_for_the_raised_edge() {
        // The acting agent from the event's actor metadata is the RAISED source,
        // matching the DecisionMade DECIDED fold. It takes precedence over `by`.
        let p = Projector::open(":memory:").unwrap();
        let payload = serde_json::json!({
            "id": "f1", "summary": "x", "about": ["a.rs"],
        });
        let mut e = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        e.meta
            .insert(META_ACTOR.to_string(), "adversary".to_string());
        p.apply(&e).unwrap();
        let g = p.subgraph(&["f1".to_string()], 2).unwrap();
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_RAISED && x.from == "adversary" && x.to == "f1"),
            "RAISED(adversary -> f1) must come from the event actor"
        );
    }

    #[test]
    fn review_finding_fold_is_idempotent_per_position() {
        // A replayed ReviewFinding (same position) must not double the ABOUT edge.
        let p = Projector::open(":memory:").unwrap();
        let payload = serde_json::json!({
            "id": "f1", "by": "lens", "summary": "x", "about": ["a.rs"],
        });
        for _ in 0..2 {
            let mut e = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&payload).unwrap());
            e.position = 1; // same position, replayed
            p.apply(&e).unwrap();
        }
        let g = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        let about = g
            .edges
            .iter()
            .filter(|x| x.rel == REL_ABOUT && x.from == "f1")
            .count();
        assert_eq!(
            about, 1,
            "a replayed finding must not double the ABOUT edge"
        );
    }

    #[test]
    fn unit_started_creates_assigned_to_and_blocks() {
        let p = Projector::open(":memory:").unwrap();
        let payload =
            serde_json::json!({"unit": "u2", "criterion": "c", "agent": "impl", "needs": ["u1"]});
        let mut e = Event::new(TYPE_UNIT_STARTED, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        p.apply(&e).unwrap();
        let g = p.subgraph(&["u2".to_string()], 2).unwrap();
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_ASSIGNED_TO && x.from == "u2" && x.to == "impl"),
            "ASSIGNED_TO(u2 -> impl)"
        );
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_BLOCKS && x.from == "u1" && x.to == "u2"),
            "BLOCKS(u1 -> u2)"
        );
    }

    #[test]
    fn aliases_collapse_synonyms_onto_one_node() {
        let p = Projector::open(":memory:").unwrap();
        let alias = serde_json::json!({"alias": "the editor", "canonical": "content-editor"});
        let mut ae = Event::new(TYPE_ALIAS_DEFINED, serde_json::to_vec(&alias).unwrap());
        ae.position = 1;
        p.apply(&ae).unwrap();
        apply_decision(&p, 2, "d1", "x", &["the editor"], "");
        let g = p.subgraph(&["content-editor".to_string()], 2).unwrap();
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_GOVERNS && x.from == "d1" && x.to == "content-editor"),
            "the alias must collapse 'the editor' onto 'content-editor'"
        );
        assert_eq!(
            p.resolve("the editor").unwrap().as_deref(),
            Some("content-editor")
        );
    }

    #[test]
    fn alias_unresolved_creates_a_node_marked_for_merge() {
        let p = Projector::open(":memory:").unwrap();
        let payload = serde_json::json!({"mention": "some thing"});
        let mut e = Event::new(TYPE_ALIAS_UNRESOLVED, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        p.apply(&e).unwrap();
        let g = p.subgraph(&["some thing".to_string()], 1).unwrap();
        let n = g
            .nodes
            .iter()
            .find(|n| n.id == "some thing")
            .expect("a node is created for an unresolved mention");
        assert_eq!(n.attrs.get("unresolved").map(String::as_str), Some("true"));
    }

    #[test]
    fn edge_valid_from_is_the_event_valid_time() {
        let p = Projector::open(":memory:").unwrap();
        let vf = std::time::UNIX_EPOCH + std::time::Duration::from_secs(500);
        let payload = serde_json::json!({"id": "d1", "summary": "x", "governs": ["mod.rs"]});
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        e.valid_from = vf;
        p.apply(&e).unwrap();
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let edge = g.edges.iter().find(|x| x.rel == REL_GOVERNS).unwrap();
        assert_eq!(
            edge.valid_from, 500_000_000_000,
            "the edge valid_from is the event's valid_from in nanos"
        );
    }
}
