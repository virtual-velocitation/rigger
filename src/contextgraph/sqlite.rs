//! The default context-graph projector: it folds the event log into bi-temporal
//! node and edge tables in a local SQLite file and answers Subgraph and Resolve.
//! A single connection behind a mutex serializes the read-then-write of apply.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::{
    Edge, Error, Graph, Node, Projection, KIND_AGENT, KIND_ARCH_DECISION, KIND_ARTIFACT,
    KIND_CODE_ENTITY, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, KIND_FINDING, KIND_GATE,
    KIND_HANDBOOK_RULE, KIND_LESSON, KIND_RATIONALE, KIND_UNIT, META_ACTOR, REL_ABOUT,
    REL_ASSIGNED_TO, REL_BLOCKS, REL_CALLS, REL_CONSTRAINS, REL_CONTAINS, REL_DECIDED,
    REL_DOC_REFERENCES, REL_EXPLAINS, REL_GATED_BY, REL_GOVERNS, REL_RAISED, REL_REFERENCES,
    REL_SPECIFIES, REL_SUPERSEDES, REL_TOUCHES, TIER_AMBIGUOUS, TIER_EXTRACTED, TIER_INFERRED,
    TYPE_ALIAS_DEFINED, TYPE_ALIAS_UNRESOLVED, TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE,
    TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED, TYPE_EDGE_INFERRED, TYPE_FILE_TOUCHED,
    TYPE_GATE_VERDICT, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING, TYPE_UNIT_INTEGRATED,
    TYPE_UNIT_STARTED,
};
use crate::eventstore::{Event, Position};
use crate::spawn::{SpawnResult, TYPE_SPAWN_RESULT};

const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
CREATE TABLE IF NOT EXISTS nodes (
  id TEXT NOT NULL, kind TEXT NOT NULL, attrs TEXT,
  project TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (id, project)
);
CREATE TABLE IF NOT EXISTS edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
  valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL,
  project TEXT NOT NULL DEFAULT '',
  tier TEXT NOT NULL DEFAULT 'extracted'
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
CREATE TABLE IF NOT EXISTS aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS applied (position INTEGER PRIMARY KEY);
";

/// Projector is the SQLite-backed Projection.
///
/// `project` is the plain project string `Namespaced::new` uses to build the `proj-<id>-`
/// stream prefix (spec 28): every node and edge this projector folds is stamped with it, so a
/// single shared backend can hold many projects without their nodes/edges ever mixing. It is
/// injected at construction, never derived a second way - the SAME identity that namespaces
/// the streams is the ONE source of truth for project scope on the graph.
pub struct Projector {
    conn: Mutex<Connection>,
    project: String,
}

impl Projector {
    /// Open (or create) the graph at `path`, scoped to `project` - the plain project string
    /// that namespaces this project's streams (`project_identity` / `StoreLocation::identity`).
    /// Every fold stamps that scope on each node and edge. A pre-spec-28 graph.db (no `project`
    /// column) is migrated in place, backfilling its existing rows with this identity, so a
    /// single-project deployment behaves exactly as before.
    pub fn open(path: &str, project: &str) -> Result<Self, Error> {
        let conn = Connection::open(path).map_err(be)?;
        conn.execute_batch(SCHEMA).map_err(be)?;
        migrate_project_scope(&conn, project)?;
        migrate_edge_tier(&conn)?;
        Ok(Projector {
            conn: Mutex::new(conn),
            project: project.to_string(),
        })
    }

    /// Prune the given nodes and every edge that touches them from the graph, returning the
    /// number of nodes actually removed (spec 21, unit 2). This is the single graph-mutation
    /// authority `rigger reset --runs` uses to shed dead-run noise: the composition root
    /// derives the superseded / pre-boundary decision and finding node ids from the ONE
    /// run-attribution primitive (`run::run_attribution` + `run::current_run_id`) and passes
    /// them here. A `LessonLearned` node is never in that set (a lesson is exempt from
    /// attribution) and the active run's nodes are never in it, so this deletes EXACTLY what
    /// it is given - the keep invariant (every lesson plus the active run, including an id
    /// reused across a dead run AND the active run) is the caller's derivation, this is only
    /// the mutation.
    ///
    /// The `applied` position ledger is left UNTOUCHED, so a later replay of the same events
    /// is a no-op that never resurrects a pruned node: the prune drops from the graph WITHOUT
    /// wiping the store, which is exactly `reset --runs`'s contract. Both deletes run in one
    /// transaction so a pruned node never outlives its edges (or the reverse) on a crash.
    pub fn prune(&self, node_ids: &[String]) -> Result<usize, Error> {
        if node_ids.is_empty() {
            return Ok(0);
        }
        let ids_json = serde_json::to_string(node_ids).map_err(be)?;
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction().map_err(be)?;
        // Delete every edge referencing a pruned node from EITHER end - a superseded
        // decision's DECIDED/GOVERNS/SUPERSEDES edges and a finding's ABOUT/RAISED edges,
        // whether currently valid or already invalidated - so no edge dangles to a gone node.
        // Scoped to THIS project (spec 28 criterion 3): on a shared backend another project may
        // hold an edge that shares a from_id/to_id with a pruned id, and `reset --runs` must
        // never touch it - the SAME injected project the fold stamps is the ONE scope.
        tx.execute(
            "DELETE FROM edges
             WHERE (from_id IN (SELECT value FROM json_each(?1))
                 OR to_id IN (SELECT value FROM json_each(?1)))
               AND project = ?2",
            params![ids_json, self.project],
        )
        .map_err(be)?;
        // Scoped identically: the composite (id, project) key lets the SAME id live under many
        // projects, so pruning project P's dead-run node leaves project Q's same-id node intact.
        let removed = tx
            .execute(
                "DELETE FROM nodes
                 WHERE id IN (SELECT value FROM json_each(?1)) AND project = ?2",
                params![ids_json, self.project],
            )
            .map_err(be)?;
        tx.commit().map_err(be)?;
        Ok(removed)
    }

    /// Re-scope every node and edge tagged `from` to `to`, in ONE transaction, returning the
    /// number of NODES moved. This is the graph analog of [`Store::rename_stream_prefix`] for
    /// the spec-09 identity migration (spec 28 GC5 backward-compat): a single-project deployment
    /// runs under its basename identity, folding graph rows tagged with it, then mints
    /// `.rigger/project.id`; the migration renames its event streams `proj-<legacy>-` ->
    /// `proj-<minted>-`, but because the graph folds incrementally the renamed streams are NEVER
    /// re-folded, so the pre-mint rows keep the legacy scope. Re-keying them to the minted
    /// identity keeps the read filter (criterion 2) returning that history, so the deployment
    /// behaves EXACTLY as before the mint. The SAME injected identity that namespaces the
    /// streams stays the ONE source of truth for the graph's project scope - this only re-derives
    /// it onto the pre-mint rows, never a second source of truth.
    ///
    /// The caller (the identity migration) re-keys BEFORE recording the migration decision, so
    /// the minted scope is still empty on the graph and the composite `(id, project)` primary key
    /// never collides. A no-op returning 0 when nothing is tagged `from`, so a re-open after the
    /// migration re-keys nothing (idempotent, mirroring `rename_stream_prefix`).
    pub fn migrate_project(&self, from: &str, to: &str) -> Result<usize, Error> {
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction().map_err(be)?;
        // Edges and nodes re-key in the same transaction so a crash never leaves an edge under
        // the old scope while its endpoint node moved (or the reverse) on a shared backend.
        tx.execute(
            "UPDATE edges SET project = ?2 WHERE project = ?1",
            params![from, to],
        )
        .map_err(be)?;
        let moved = tx
            .execute(
                "UPDATE nodes SET project = ?2 WHERE project = ?1",
                params![from, to],
            )
            .map_err(be)?;
        tx.commit().map_err(be)?;
        Ok(moved)
    }
}

/// Additive backward-compat migration (spec 28, criterion 1). A graph.db created before the
/// project scope existed has `nodes(id, kind, attrs)` and `edges(..., source)` with no
/// `project` column. Bring it to the scoped shape WITHOUT wiping it: recreate `nodes` with the
/// composite `(id, project)` primary key (so the SAME node id can coexist across projects on a
/// shared backend) and add `project` to `edges`, BACKFILLING every existing row with the
/// opener's own identity. So an upgraded single-project graph.db reads identically once the
/// read filter (criterion 2) lands - its rows carry its own project. Idempotent: a fresh or
/// already-migrated db already has the column and both arms are skipped.
fn migrate_project_scope(conn: &Connection, project: &str) -> Result<(), Error> {
    if !column_exists(conn, "nodes", "project")? {
        // SQLite cannot alter a primary key in place, so copy the old rows through a renamed
        // table into the new composite-keyed `nodes`, stamping the opener's project on each.
        conn.execute("ALTER TABLE nodes RENAME TO nodes_pre_project", [])
            .map_err(be)?;
        conn.execute_batch(
            "CREATE TABLE nodes (
               id TEXT NOT NULL, kind TEXT NOT NULL, attrs TEXT,
               project TEXT NOT NULL DEFAULT '',
               PRIMARY KEY (id, project)
             );",
        )
        .map_err(be)?;
        conn.execute(
            "INSERT INTO nodes (id, kind, attrs, project)
             SELECT id, kind, attrs, ?1 FROM nodes_pre_project",
            params![project],
        )
        .map_err(be)?;
        conn.execute("DROP TABLE nodes_pre_project", [])
            .map_err(be)?;
    }
    if !column_exists(conn, "edges", "project")? {
        conn.execute(
            "ALTER TABLE edges ADD COLUMN project TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(be)?;
        conn.execute("UPDATE edges SET project = ?1", params![project])
            .map_err(be)?;
    }
    Ok(())
}

/// Additive backward-compat migration for the confidence tier (spec 29a, addendum 6.2). A graph.db
/// written before the tier existed has an `edges` table with no `tier` column. Bring it to the
/// tiered shape WITHOUT wiping it: add the column, defaulting every existing row to
/// [`TIER_EXTRACTED`]. That default IS the correct backfill - every pre-29a edge is a dev-loop fact
/// (DECIDED / GOVERNS / ABOUT / SUPERSEDES / ...), which addendum 6.2 tags EXTRACTED - so unlike the
/// project backfill this needs no second UPDATE. Idempotent: a fresh or already-migrated db already
/// has the column (the `SCHEMA` literal carries it) and the arm is skipped. The literal must match
/// [`TIER_EXTRACTED`], which `tier_default_matches_the_extracted_const` pins.
fn migrate_edge_tier(conn: &Connection) -> Result<(), Error> {
    if !column_exists(conn, "edges", "tier")? {
        conn.execute(
            "ALTER TABLE edges ADD COLUMN tier TEXT NOT NULL DEFAULT 'extracted'",
            [],
        )
        .map_err(be)?;
    }
    Ok(())
}

/// Whether `table` (a trusted schema-literal name, never caller input) has a column named
/// `col`, via `PRAGMA table_info` - so [`migrate_project_scope`] fires exactly once and leaves
/// a fresh or already-migrated db untouched.
fn column_exists(conn: &Connection, table: &str, col: &str) -> Result<bool, Error> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(be)?;
    let mut rows = stmt.query([]).map_err(be)?;
    while let Some(row) = rows.next().map_err(be)? {
        let name: String = row.get(1).map_err(be)?;
        if name == col {
            return Ok(true);
        }
    }
    Ok(false)
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
            fold(&tx, e, &self.project)?;
        }
        tx.commit().map_err(be)?;
        Ok(())
    }

    fn subgraph(&self, seed: &[String], depth: i64) -> Result<Graph, Error> {
        let seed_json = serde_json::to_string(seed).map_err(be)?;
        let conn = self.conn.lock().unwrap();

        // Read isolation (spec 28, criterion 2): every read is scoped to `self.project` - the
        // SAME plain project string `Namespaced::new` uses for the `proj-<id>-` stream prefix
        // and the write tag (criterion 1) stamps on each row. So on a shared backend holding
        // many projects, a seed id present in two projects returns ONLY the current project's
        // neighborhood. This mirrors, for the graph, what `Namespaced::scope_filter` does for
        // streams - it is one authority keyed on the injected identity, never a second source
        // of truth. The traversal itself is scoped (`e.project`), so it never walks another
        // project's edge into a node it does not own, and the node/edge fetches are scoped so a
        // same-id row from another project is never returned.
        let mut reach = conn
            .prepare(
                "WITH RECURSIVE reach(id, depth) AS (
                   SELECT value, 0 FROM json_each(?1)
                   UNION
                   SELECT CASE WHEN e.from_id = r.id THEN e.to_id ELSE e.from_id END, r.depth + 1
                   FROM reach r JOIN edges e
                     ON (e.from_id = r.id OR e.to_id = r.id)
                        AND e.valid_to IS NULL AND e.project = ?3
                   WHERE r.depth < ?2
                 )
                 SELECT DISTINCT id FROM reach",
            )
            .map_err(be)?;
        let ids: Vec<String> = reach
            .query_map(params![seed_json, depth, self.project], |r| r.get(0))
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;
        if ids.is_empty() {
            return Ok(Graph::default());
        }
        let ids_json = serde_json::to_string(&ids).map_err(be)?;

        let mut nstmt = conn
            .prepare(
                "SELECT id, kind, attrs FROM nodes
                 WHERE id IN (SELECT value FROM json_each(?1)) AND project = ?2",
            )
            .map_err(be)?;
        let nodes: Vec<Node> = nstmt
            .query_map(params![ids_json, self.project], row_to_node)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;

        let mut estmt = conn
            .prepare(
                "SELECT from_id, to_id, rel, valid_from, source, tier FROM edges
                 WHERE valid_to IS NULL
                   AND project = ?2
                   AND from_id IN (SELECT value FROM json_each(?1))
                   AND to_id IN (SELECT value FROM json_each(?1))",
            )
            .map_err(be)?;
        let edges: Vec<Edge> = estmt
            .query_map(params![ids_json, self.project], row_to_edge)
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
        // Read isolation (spec 28, criterion 2): the node-existence fallback is a read of the
        // nodes table, so it answers for `self.project` only - a node id living solely under
        // another project on a shared backend must not resolve here (no cross-project
        // false-positive existence). The alias arm above stays unscoped: the `aliases` table
        // carries no project column and is shared, so only this node lookup is scoped.
        conn.query_row(
            "SELECT id FROM nodes WHERE id = ?1 AND project = ?2",
            params![mention, self.project],
            |r| r.get(0),
        )
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
        tier: r.get(5)?,
    })
}

fn fold(tx: &Transaction, e: &Event, project: &str) -> Result<(), Error> {
    // The edge's bi-temporal valid-time is when the fact became true (the event's
    // caller-supplied valid_from), not the ingest time.
    let at = to_nanos(e.valid_from);
    match e.type_.as_str() {
        TYPE_DECISION_MADE => {
            let d: super::DecisionMade = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &d.id,
                KIND_DECISION,
                &[("summary", &d.summary)],
                project,
            )?;
            // DECIDED: the acting agent (from event metadata) made this decision.
            if let Some(actor) = e.meta.get(META_ACTOR).filter(|a| !a.is_empty()) {
                ensure_node(tx, actor, KIND_AGENT, &[], project)?;
                add_edge(
                    tx,
                    actor,
                    &d.id,
                    REL_DECIDED,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
            for path in &d.governs {
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[], project)?;
                add_edge(
                    tx,
                    &d.id,
                    &canonical,
                    REL_GOVERNS,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
            if !d.supersedes.is_empty() {
                ensure_node(tx, &d.supersedes, KIND_DECISION, &[], project)?;
                add_edge(
                    tx,
                    &d.id,
                    &d.supersedes,
                    REL_SUPERSEDES,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
                // Invalidate (never delete) the governing edges the superseded decision
                // asserted - scoped to this project's edges, so a shared-backend fold never
                // touches another project's edge that happens to share the from_id.
                tx.execute(
                    "UPDATE edges SET valid_to = ?1
                     WHERE from_id = ?2 AND rel = ?3 AND valid_to IS NULL AND project = ?4",
                    params![at, d.supersedes, REL_GOVERNS, project],
                )
                .map_err(be)?;
            }
        }
        TYPE_FILE_TOUCHED => {
            let f: super::FileTouched = serde_json::from_slice(&e.data).map_err(be)?;
            let path = resolve_in_tx(tx, &f.path);
            ensure_node(tx, &path, KIND_ARTIFACT, &[], project)?;
            if !f.by.is_empty() {
                ensure_node(tx, &f.by, KIND_AGENT, &[], project)?;
                add_edge(
                    tx,
                    &f.by,
                    &path,
                    REL_TOUCHES,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
        }
        TYPE_GATE_VERDICT => {
            let g: super::GateVerdict = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &g.gate,
                KIND_GATE,
                &[("pass", &g.pass.to_string())],
                project,
            )?;
            if !g.artifact.is_empty() {
                let artifact = resolve_in_tx(tx, &g.artifact);
                ensure_node(tx, &artifact, KIND_ARTIFACT, &[], project)?;
                add_edge(
                    tx,
                    &artifact,
                    &g.gate,
                    REL_GATED_BY,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
        }
        TYPE_UNIT_STARTED => {
            let u: super::UnitStarted = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &u.unit,
                KIND_UNIT,
                &[("criterion", &u.criterion), ("status", "started")],
                project,
            )?;
            // ASSIGNED_TO: the unit is assigned to its agent.
            if !u.agent.is_empty() {
                ensure_node(tx, &u.agent, KIND_AGENT, &[], project)?;
                add_edge(
                    tx,
                    &u.unit,
                    &u.agent,
                    REL_ASSIGNED_TO,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
            // BLOCKS: each dependency blocks this unit until it lands.
            for need in &u.needs {
                ensure_node(tx, need, KIND_UNIT, &[], project)?;
                add_edge(
                    tx,
                    need,
                    &u.unit,
                    REL_BLOCKS,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
        }
        TYPE_UNIT_INTEGRATED => {
            let u: super::UnitIntegrated = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &u.unit,
                KIND_UNIT,
                &[("commit", &u.commit), ("status", "integrated")],
                project,
            )?;
            // Disposition-expiry (spec 25, criterion 2 - the UPHELD-AND-ADDRESSED trigger's
            // INVALIDATE half): integrating a unit ADDRESSES every finding its review upheld,
            // so those findings are now resolved. The adjudicator's earlier SpawnResult marked
            // each upheld finding-of-this-unit (disposition=upheld, unit=<this unit>); expire
            // them now through the same shared authority the discard trigger uses. A finding
            // upheld for a DIFFERENT unit, or upheld here but re-raised under a later run (a
            // re-raise re-runs ensure_node, which COALESCE-overwrites the whole attrs and so
            // clears the marker), carries no matching mark and is untouched - keeping the
            // invalidation run-scoped by construction. Collect the marked ids deterministically
            // (ORDER BY id) before mutating so the fold order never varies.
            let marked: Vec<String> = {
                let mut stmt = tx
                    .prepare(
                        "SELECT id FROM nodes
                          WHERE kind = ?1
                            AND json_extract(attrs, '$.disposition') = 'upheld'
                            AND json_extract(attrs, '$.unit') = ?2
                            AND project = ?3
                          ORDER BY id",
                    )
                    .map_err(be)?;
                let ids = stmt
                    .query_map(params![KIND_FINDING, u.unit, project], |r| {
                        r.get::<_, String>(0)
                    })
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                ids
            };
            for fid in &marked {
                invalidate_finding_edges(tx, fid, at, project)?;
            }
        }
        TYPE_LESSON_LEARNED => {
            let l: super::LessonLearned = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &l.id, KIND_LESSON, &[("summary", &l.summary)], project)?;
            for path in &l.about {
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[], project)?;
                add_edge(
                    tx,
                    &l.id,
                    &canonical,
                    REL_ABOUT,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
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
                project,
            )?;
            let raiser = e
                .meta
                .get(META_ACTOR)
                .filter(|a| !a.is_empty())
                .map(String::as_str)
                .unwrap_or(f.by.as_str());
            if !raiser.is_empty() {
                ensure_node(tx, raiser, KIND_AGENT, &[], project)?;
                add_edge(
                    tx,
                    raiser,
                    &f.id,
                    REL_RAISED,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
            for path in &f.about {
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[], project)?;
                add_edge(
                    tx,
                    &f.id,
                    &canonical,
                    REL_ABOUT,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
        }
        TYPE_CODE_ENTITY_EXTRACTED => {
            // Spec 29a criterion 1: one definition the extraction pass emitted. Fold it into a
            // code-entity node hung off its file container node, so code structure lives in the
            // event-sourced projection, not a mutable side index. ALWAYS compiled: the light lane
            // folds this with the extraction pass absent, which is why the node kinds and this arm
            // live outside the `symbols` feature. Project-scoped like every arm (spec 28): the
            // file, the entity, and the edge all carry the injected project. The file path is
            // alias-resolved exactly as the artifact-producing arms resolve theirs, so the file
            // container is the SAME node those arms build for the path (one-graph identity), never
            // a parallel node that only coincidentally shares a literal string.
            let c: super::CodeEntityExtracted = serde_json::from_slice(&e.data).map_err(be)?;
            let file = resolve_in_tx(tx, &c.file);
            // Supersede-on-re-extract (criterion 3): the FIRST event of an extraction batch retires
            // the file's prior structural edges before this batch folds its own, so a re-extraction
            // REPLACES rather than accretes. A no-op on the initial extraction (no prior edges).
            if c.fresh {
                supersede_file_edges(tx, &file, at, project)?;
            }
            ensure_node(tx, &file, KIND_FILE, &[("lang", &c.lang)], project)?;
            let entity = code_entity_id(&file, &c.name);
            let line = c.line.to_string();
            ensure_node(
                tx,
                &entity,
                KIND_CODE_ENTITY,
                &[
                    ("name", &c.name),
                    ("kind", &c.kind),
                    ("line", &line),
                    ("lang", &c.lang),
                ],
                project,
            )?;
            // CONTAINS: the file container node holds this definition. A definition's containment
            // is the most explicit structural fact there is, so it folds at the EXTRACTED tier
            // (spec 29a, addendum 6.2).
            add_edge(
                tx,
                &file,
                &entity,
                REL_CONTAINS,
                at,
                e.position,
                project,
                TIER_EXTRACTED,
            )?;
            // Convergent tier upgrade (spec 29a criterion 2): a reference to THIS name that folded
            // BEFORE this definition existed was tiered AMBIGUOUS (grep-visible-only - no definition
            // was known yet). Now that the definition IS known, any such reference from ANOTHER file
            // is a derived / transitive link, so promote it AMBIGUOUS -> INFERRED. This mirrors the
            // one-writer kind-promotion above: it makes the tier a pure function of the FINAL log,
            // independent of whether a reference or its definition folds first, so a rebuild
            // re-derives byte-identical tiers. Matched exactly by the name suffix of the reference's
            // target id (`<file>::<name>`, and a file path never contains `::`), never by a
            // wildcard, so a symbol whose name contains any character still matches precisely. A
            // same-file reference already folded EXTRACTED (definitions emit before references), so
            // excluding this definition's own entity id leaves it untouched, never demoting it.
            //
            // The caller-attributed CALLS edge (spec 37) shares the callee `target` and tier of its
            // REFERENCES twin, so it is promoted in the SAME upgrade (`rel IN (REFERENCES, CALLS)`):
            // one tier authority evolves both structural edges together, never a forked behavior
            // where the CALLS edge lags its sibling's confidence. A same-file CALLS to this
            // definition's own entity is excluded by the same `to_id != entity` guard.
            tx.execute(
                "UPDATE edges SET tier = ?1
                   WHERE rel IN (?2, ?7) AND tier = ?3 AND project = ?4 AND valid_to IS NULL
                     AND to_id != ?5
                     AND substr(to_id, instr(to_id, '::') + 2) = ?6",
                params![
                    TIER_INFERRED,
                    REL_REFERENCES,
                    TIER_AMBIGUOUS,
                    project,
                    entity,
                    c.name,
                    REL_CALLS
                ],
            )
            .map_err(be)?;
        }
        TYPE_EDGE_INFERRED => {
            // Spec 29a criterion 1: one reference the extraction pass emitted. Fold it into a
            // structural REFERENCES edge from the referencing file to the referenced symbol's
            // file-scoped code-entity id. When that name is defined in the same file, the edge
            // lands on its definition entity (a real intra-file reference); otherwise `ensure_node`
            // creates a bare code-entity node for the referenced name so the edge never dangles -
            // cross-file name resolution is out of this criterion's scope. The empty attr set means
            // a reference never overwrites a definition's attrs (the `ensure_node` COALESCE keeps
            // the existing ones). ALWAYS compiled, project-scoped like every arm. The file path is
            // alias-resolved like the artifact-producing arms, so the referencing file node is the
            // SAME one-graph node (see the definition arm above).
            let r: super::EdgeInferred = serde_json::from_slice(&e.data).map_err(be)?;
            let file = resolve_in_tx(tx, &r.file);
            // Supersede-on-re-extract (criterion 3): a refs-only file (no definitions) carries the
            // batch boundary on its first reference; retire the file's prior structural edges before
            // folding this one, so the two fold arms share one supersede authority.
            if r.fresh {
                supersede_file_edges(tx, &file, at, project)?;
            }
            ensure_node(tx, &file, KIND_FILE, &[("lang", &r.lang)], project)?;
            let target = code_entity_id(&file, &r.name);
            // REFERENCES (spec 29a criterion 2): the file references this symbol, at the confidence
            // tier its resolution earns. The tier is read BEFORE `ensure_node` creates the bare
            // target, so a bare target this reference is about to create never miscounts as a
            // definition. The definition arm's convergent upgrade covers the reverse fold order.
            let tier = reference_tier(tx, &target, &r.name, project)?;
            ensure_node(tx, &target, KIND_CODE_ENTITY, &[], project)?;
            add_edge(
                tx,
                &file,
                &target,
                REL_REFERENCES,
                at,
                e.position,
                project,
                tier,
            )?;
            // Caller-attributed CALLS edge (spec 37): when extraction attributed this reference to an
            // enclosing definition, ADD `<file>::<caller> --CALLS--> <callee>` ALONGSIDE the file
            // REFERENCES edge above, so one `subgraph` around the callee answers "who calls it" by
            // FUNCTION, not merely "referenced from which file". Purely additive: a caller-less
            // reference (a top-level `use`/import) folds no CALLS edge, exactly today's behavior.
            // Callee resolution is UNCHANGED - the SAME `target` and `tier` the REFERENCES edge uses,
            // so the CALLS edge is a faithful caller-keyed twin (the definition arm's convergent
            // upgrade promotes both rels together, keeping the twin's tier in lock-step). The caller
            // entity node is `ensure_node`d bare like the target: in real extraction the enclosing
            // definition folded first (defs emit before refs) so this is a no-op that keeps its
            // attrs, and a reverse fold order still never leaves the CALLS edge dangling.
            if let Some(caller) = &r.caller {
                let caller_id = code_entity_id(&file, caller);
                ensure_node(tx, &caller_id, KIND_CODE_ENTITY, &[], project)?;
                add_edge(
                    tx, &caller_id, &target, REL_CALLS, at, e.position, project, tier,
                )?;
            }
        }
        TYPE_DOC_CONCEPT_EXTRACTED => {
            // Spec 29b criterion 1: one design-intent concept the doc extraction pass emitted. Fold
            // it into a design-doc / arch-decision / handbook-rule / rationale node, so the
            // design-intent layer lives in the event-sourced projection alongside the code half -
            // the reference architecture becomes a set of queryable nodes in the very graph it
            // specifies. ALWAYS compiled: the light lane folds a design-intent log with the
            // extraction pass absent, which is why the node kinds and this arm live outside the
            // feature that gates the extraction, mirroring the 29a CodeEntityExtracted arm.
            //
            // The four kinds are matched exactly; a payload carrying any other kind string folds
            // nothing (defensive - the emit only ever produces these four). Project-scoped like
            // every arm (spec 28). The id is alias-resolved exactly as the artifact-producing arms
            // resolve their paths, so a design-doc whose id is a doc path is the SAME one-graph node
            // that a decision GOVERNS or a lesson is ABOUT (addendum 6.1 single id space) - the
            // `ensure_node` promotion below settles which kind wins.
            let c: super::DocConceptExtracted = serde_json::from_slice(&e.data).map_err(be)?;
            let kind = match c.kind.as_str() {
                KIND_DESIGN_DOC => KIND_DESIGN_DOC,
                KIND_ARCH_DECISION => KIND_ARCH_DECISION,
                KIND_HANDBOOK_RULE => KIND_HANDBOOK_RULE,
                KIND_RATIONALE => KIND_RATIONALE,
                _ => return Ok(()),
            };
            let id = resolve_in_tx(tx, &c.id);
            ensure_node(
                tx,
                &id,
                kind,
                &[("title", &c.title), ("doc", &c.doc)],
                project,
            )?;
        }
        TYPE_DOC_LINK_EXTRACTED => {
            // Spec 29b criterion 2: one design-intent link the doc extraction pass emitted. Fold it
            // into a typed design-intent edge - design-doc --SPECIFIES--> code, arch-decision
            // --CONSTRAINS--> code, handbook-rule --GOVERNS--> code (REUSING REL_GOVERNS, never a
            // second governs relation), rationale --explains--> code, and design-doc --references-->
            // doc - so the design-intent layer's links live in the event-sourced projection
            // alongside the code half; a subgraph traversal from a touched file then reaches the RA
            // section that designed it and the decision that constrains it. ALWAYS compiled: the
            // light lane folds a design-intent log with the extraction pass absent, which is why the
            // edge relations and this arm live outside the feature that gates the extraction,
            // mirroring the 29a EdgeInferred arm.
            //
            // The five relations are matched exactly; a payload carrying any other relation string
            // folds nothing (defensive - the emit only ever produces these five), mirroring the
            // concept arm's kind guard. Every design-intent link is an explicit design fact recorded
            // on the log, so it folds at TIER_EXTRACTED (addendum 6.2 - the precise seed). Both
            // endpoints are alias-resolved and ensured exactly as the artifact-producing arms
            // resolve their paths, so the edge lands on the SAME one-graph nodes a decision GOVERNS,
            // a lesson is ABOUT, code was extracted from (spec 29a), or design intent was ingested
            // into (criterion 1, addendum 6.1 single id space) - never a parallel node that only
            // coincidentally shares a literal string. The endpoints are ensured as the generic
            // KIND_ARTIFACT role: a design-doc from-node folded by criterion 1 keeps its specific
            // kind (ensure_node never demotes), a bare target promotes to a file / design-doc when
            // its own extraction folds, and the edge never dangles when it folds before its
            // endpoints. This is the single edge-fold authority for design-intent links; c2 owns the
            // edge relations, criterion 1 owns the node kinds.
            let l: super::DocLinkExtracted = serde_json::from_slice(&e.data).map_err(be)?;
            let rel = match l.rel.as_str() {
                REL_SPECIFIES => REL_SPECIFIES,
                REL_CONSTRAINS => REL_CONSTRAINS,
                REL_GOVERNS => REL_GOVERNS,
                REL_EXPLAINS => REL_EXPLAINS,
                REL_DOC_REFERENCES => REL_DOC_REFERENCES,
                _ => return Ok(()),
            };
            let from = resolve_in_tx(tx, &l.from);
            let to = resolve_in_tx(tx, &l.to);
            ensure_node(tx, &from, KIND_ARTIFACT, &[], project)?;
            ensure_node(tx, &to, KIND_ARTIFACT, &[], project)?;
            add_edge(tx, &from, &to, rel, at, e.position, project, TIER_EXTRACTED)?;
        }
        TYPE_SPAWN_RESULT => {
            // Disposition-expiry (spec 25, criterion 1 - the DISCARD trigger): an
            // adjudicator's recorded result is where a review's findings are RESOLVED. A
            // finding the adjudicator NAMES in its verdict line's `discarded` array is
            // DISCARDED, so invalidate (set valid_to, never delete - mirroring the
            // decision-supersession arm above) its RAISED / ABOUT edges; the live `subgraph`
            // filter (valid_to IS NULL) then prunes it so agents ground on LIVE findings only.
            //
            // Keying on the EXPLICIT `discarded` finding ids - a field production sets on
            // every real finding (`data.id`, stamped back into the verdict by the
            // adjudicator) - is deliberate on two counts. (1) It fires against what
            // production records: a real finding carries NO `$.unit` attr (cmd_emit / the MCP
            // server stamp only `meta.spawn`, and `ReviewFinding.unit` defaults empty), so a
            // fold keyed on `json_extract(attrs,'$.unit')` would match nothing and expire
            // nothing. (2) It never over-invalidates: the discard is NOT the complement of
            // `upheld` (56/234 adjudications approve with no `upheld` at all), so a verdict
            // that omits `upheld` never sweeps a review's still-open findings, and a reject's
            // own motivating findings stay live for the remediation unless the adjudicator
            // explicitly discarded them. The disposition is read through the single
            // `SpawnResult::adjudication` authority the review-quality metric also reads (it
            // self-gates on the adjudicator role, so a non-adjudicator result yields nothing),
            // keeping the graph and the metric on one story. An unparseable result
            // graceful-skips (mirroring the metrics fold), so one malformed event never wedges
            // a whole rebuild.
            if let Ok(res) = SpawnResult::from_event(e) {
                if let Some(adj) = res.adjudication() {
                    // Determinism (no HashMap iteration): the discarded ids through a BTreeSet,
                    // so the invalidations run in a fixed id order whatever order the verdict
                    // array listed them.
                    let discarded: BTreeSet<String> = adj.discarded.into_iter().collect();
                    for fid in &discarded {
                        // Invalidate the discarded finding's provenance (RAISED) and file
                        // (ABOUT) edges through the shared single authority below.
                        invalidate_finding_edges(tx, fid, at, project)?;
                    }
                    // Disposition-expiry (spec 25, criterion 2 - the UPHELD-AND-ADDRESSED
                    // trigger's MARK half): a finding the adjudicator UPHELD is not yet
                    // resolved - it is resolved only when the unit that owns it INTEGRATES and
                    // addresses it. So MARK each upheld finding with its disposition and the
                    // unit it belongs to (the adjudicator spawn id's unit token, the same split
                    // metrics.rs reads), and let the TYPE_UNIT_INTEGRATED arm invalidate it on
                    // integration. json_set MERGES the two keys into the finding's existing
                    // attrs so its summary / by survive - an upheld-but-not-yet-integrated
                    // finding still renders live in grounding until its unit lands. The guard
                    // `kind = KIND_FINDING` keeps a stray upheld id from stamping another node.
                    let unit = res.id.split('/').next().unwrap_or(&res.id);
                    let upheld: BTreeSet<String> = adj.upheld.into_iter().collect();
                    for fid in &upheld {
                        tx.execute(
                            "UPDATE nodes
                                SET attrs = json_set(
                                    COALESCE(attrs, '{}'), '$.disposition', 'upheld', '$.unit', ?2)
                              WHERE id = ?1 AND kind = ?3 AND project = ?4",
                            params![fid, unit, KIND_FINDING, project],
                        )
                        .map_err(be)?;
                    }
                }
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
            ensure_node(
                tx,
                &a.mention,
                KIND_ARTIFACT,
                &[("unresolved", "true")],
                project,
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Invalidate (set `valid_to`, never delete - mirroring the decision-supersession arm) the
/// RAISED (provenance, into it) and ABOUT (file, out of it) edges of a RESOLVED finding, so the
/// live `subgraph` filter (`valid_to IS NULL`) stops returning it and agents ground on LIVE
/// findings only. Guarded by `EXISTS(KIND_FINDING)` so a stray id can never expire another node
/// kind's edges. This is the single edge-invalidation authority both disposition-expiry triggers
/// (spec 25) share: the DISCARD trigger on an adjudicator's result, and the UPHELD-AND-ADDRESSED
/// trigger on a unit's integration. Only edges that currently hold (`valid_to IS NULL`) are
/// touched, so an already-invalidated edge and a later run's fresh re-raise are both left alone.
fn invalidate_finding_edges(
    tx: &Transaction,
    fid: &str,
    at: i64,
    project: &str,
) -> Result<(), Error> {
    tx.execute(
        "UPDATE edges SET valid_to = ?1
         WHERE valid_to IS NULL
           AND project = ?6
           AND ((from_id = ?2 AND rel = ?3) OR (to_id = ?2 AND rel = ?4))
           AND EXISTS (SELECT 1 FROM nodes WHERE id = ?2 AND kind = ?5 AND project = ?6)",
        params![at, fid, REL_ABOUT, REL_RAISED, KIND_FINDING, project],
    )
    .map_err(be)?;
    Ok(())
}

/// Supersede-on-re-extract (spec 29a criterion 3): set `valid_to` on (never delete - mirroring the
/// decision-supersession arm and [`invalidate_finding_edges`]) every LIVE structural edge OUT of a
/// file's OWN structure - the `CONTAINS` / `REFERENCES` edges out of its container node AND the
/// `CALLS` edges out of the code entities it defines (spec 37). Called at the boundary of a file's
/// extraction batch (the `fresh` event), BEFORE that batch folds its own edges, so re-extracting a
/// changed file REPLACES its structural edges rather than accreting duplicates: a removed
/// definition's / reference's / call's edge drops from the live `subgraph` (its `valid_to` is now
/// set) while the new pass inserts fresh live edges. The old rows are retained with `valid_to`
/// stamped, so a historical / as-of query still reaches the previous graph (bi-temporal, spec 29a
/// section 6.4).
///
/// The `CONTAINS` / `REFERENCES` edges hang off `from_id = file` (the container node); a `CALLS`
/// edge instead hangs off `from_id = <file>::<caller>` (the enclosing definition entity), so it is
/// matched by an EXACT `<file>::` id prefix (`substr`, never a `LIKE`/`GLOB` whose `_`/`%`/`*`
/// wildcards a real path could contain). Both scopings retire ONLY this file's own structure: a
/// cross-file `REFERENCES` from ANOTHER file (whose `from_id` is that other file) and a cross-file
/// `CALLS` whose caller lives in another file (whose `from_id` is `<other-file>::...`) are left
/// untouched, as is a non-structural edge INTO the file (an agent `TOUCHES` it, a decision `GOVERNS`
/// it, the file `GATED_BY` a gate). Project-scoped like every fold, so a shared backend never
/// touches another project's edges. On the initial extraction this matches zero live edges (the
/// file has none yet).
fn supersede_file_edges(tx: &Transaction, file: &str, at: i64, project: &str) -> Result<(), Error> {
    tx.execute(
        "UPDATE edges SET valid_to = ?1
         WHERE valid_to IS NULL AND project = ?5
           AND (
             (from_id = ?2 AND (rel = ?3 OR rel = ?4))
             OR (rel = ?6 AND substr(from_id, 1, length(?2) + 2) = ?2 || '::')
           )",
        params![at, file, REL_CONTAINS, REL_REFERENCES, project, REL_CALLS],
    )
    .map_err(be)?;
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

/// The stable node id for a code entity (spec 29a): `<file>::<name>`. File-scoped so two files
/// defining the same name are distinct located entities, and stable across re-extraction (it
/// carries no line, which an edit shifts) so a later supersede-on-re-extract criterion can key a
/// file's edges by this from-side identity. The SAME id derives a definition's node and a
/// same-file reference's target, so a reference to a locally-defined name lands on its definition.
fn code_entity_id(file: &str, name: &str) -> String {
    format!("{file}::{name}")
}

/// The confidence tier for a REFERENCES edge (spec 29a criterion 2, addendum 6.2). Derived from how
/// the referenced `name` resolves against the graph's structural knowledge, WITHOUT moving the edge
/// (cross-file resolution stays out of scope - only the tier is classified):
///
/// - the name is DEFINED in the SAME file (the target `<file>::<name>` already carries a
///   definition's attrs) -> [`TIER_EXTRACTED`]: an explicit, resolved local reference (a call /
///   import / inherit of a known local symbol).
/// - else the name is DEFINED in ANOTHER file the graph knows (some other code-entity node carries
///   it as a definition) -> [`TIER_INFERRED`]: a derived / transitive cross-file link.
/// - else the name is defined NOWHERE known -> [`TIER_AMBIGUOUS`]: a grep-visible-only occurrence (a
///   macro body, reflection string, dynamic or external name). It is kept, never dropped, so the
///   safe superset stays a grep-superset (addendum 2.4), but tiered lowest.
///
/// A definition node is told from a bare reference target by carrying a `name` attr (the definition
/// arm sets it; the reference arm creates bare, attr-less targets). Called BEFORE the reference arm
/// `ensure_node`s its bare target, so the target this reference is about to create never miscounts
/// as its own definition. The reverse fold order - a reference folding before its cross-file
/// definition - is reconciled convergently by the definition arm's AMBIGUOUS -> INFERRED upgrade, so
/// the stored tier is a pure function of the final log, not of fold interleaving.
fn reference_tier(
    tx: &Transaction,
    target: &str,
    name: &str,
    project: &str,
) -> Result<&'static str, Error> {
    // Same-file definition: the target id already carries a definition's `name` attr.
    let same_file_def = tx
        .query_row(
            "SELECT 1 FROM nodes
              WHERE id = ?1 AND project = ?2 AND json_extract(attrs, '$.name') IS NOT NULL",
            params![target, project],
            |_| Ok(()),
        )
        .optional()
        .map_err(be)?
        .is_some();
    if same_file_def {
        return Ok(TIER_EXTRACTED);
    }
    // Cross-file definition: a code-entity in a DIFFERENT file (id != this target) carries this
    // name as a definition. A bare reference target elsewhere never matches - it has no `name` attr.
    let cross_file_def = tx
        .query_row(
            "SELECT 1 FROM nodes
              WHERE kind = ?1 AND project = ?2 AND id != ?3
                AND json_extract(attrs, '$.name') = ?4
              LIMIT 1",
            params![KIND_CODE_ENTITY, project, target, name],
            |_| Ok(()),
        )
        .optional()
        .map_err(be)?
        .is_some();
    if cross_file_def {
        Ok(TIER_INFERRED)
    } else {
        Ok(TIER_AMBIGUOUS)
    }
}

fn ensure_node(
    tx: &Transaction,
    id: &str,
    kind: &str,
    attrs: &[(&str, &str)],
    project: &str,
) -> Result<(), Error> {
    let attr_json: Option<String> = if attrs.is_empty() {
        None
    } else {
        let map: BTreeMap<&str, &str> = attrs.iter().copied().collect();
        Some(serde_json::to_string(&map).map_err(be)?)
    };
    // The project scope (spec 28) is part of the node's identity: the conflict target is
    // (id, project), so the SAME id under a DIFFERENT project is a distinct row, never an
    // upsert over another project's node.
    //
    // One-graph identity (spec 29a/29b, addendum 6.1 single id space): a rel-path is ONE node
    // whether it is reached as a touched / governed / cited artifact (KIND_ARTIFACT), as a source
    // file with extracted code structure (KIND_FILE, spec 29a), or as an ingested design-intent doc
    // (KIND_DESIGN_DOC / KIND_ARCH_DECISION / KIND_HANDBOOK_RULE / KIND_RATIONALE, spec 29b). All
    // fold into the same (id, project) row, so the kind must resolve deterministically no matter
    // which event folds first. KIND_ARTIFACT is the GENERIC role - a path merely referenced by a
    // decision / lesson / finding - so a more specific role PROMOTES it: a path only becomes a file
    // because code was extracted from it, and only becomes a design-doc because it was ingested as
    // design intent, and either PROVES what the path is. On conflict an existing bare KIND_ARTIFACT
    // is promoted to the specific incoming kind and, symmetrically, a later KIND_ARTIFACT reference
    // never DEMOTES an established specific kind - so the node's kind is a pure function of the
    // source, not of log interleaving. Only KIND_ARTIFACT promotes: its ids are the only path
    // space, and every other kind keeps first-writer-wins (their ids are distinct slug spaces -
    // decision / unit / agent / gate ids - that never collide with a path). This is the single
    // node-mutation authority, so the reconciliation lives here rather than in a second UPDATE path.
    tx.execute(
        "INSERT INTO nodes (id, kind, attrs, project) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id, project) DO UPDATE SET
             attrs = COALESCE(excluded.attrs, nodes.attrs),
             kind = CASE
                 WHEN nodes.kind = ?5 AND excluded.kind IN (?6, ?7, ?8, ?9, ?10)
                     THEN excluded.kind
                 ELSE nodes.kind
             END",
        params![
            id,
            kind,
            attr_json,
            project,
            KIND_ARTIFACT,
            KIND_FILE,
            KIND_DESIGN_DOC,
            KIND_ARCH_DECISION,
            KIND_HANDBOOK_RULE,
            KIND_RATIONALE,
        ],
    )
    .map_err(be)?;
    Ok(())
}

/// Assert a live edge, UPSERT-LIVE (spec 40): at most one live edge per
/// `(from_id, to_id, rel, tier, project)`. Every fold arm re-asserts relationships over time - a
/// `FileTouched` refolds `agent --TOUCHES--> file` on EVERY touch, a re-run refolds `GOVERNS` /
/// `ABOUT`, and so on. A bare `INSERT ... valid_to = NULL` therefore accreted an identical live
/// row per fold, bloating the graph and the grounding slice injected into every prompt. So before
/// inserting, look for the existing LIVE edge with this exact key: if one is present, record the
/// latest assertion in place - bump `source` to the newest position, keep the EARLIEST `valid_from`
/// (the fact has held since it first became true) - and add NO row; otherwise INSERT as before.
///
/// Keyed on LIVE edges only (`valid_to IS NULL`), so it never suppresses a legitimate re-assertion
/// AFTER an invalidation: a superseded `GOVERNS` (its `valid_to` set) that is later re-asserted
/// correctly folds a NEW live edge. Dedup collapses only EXACT duplicates (identical
/// `from`/`to`/`rel`/`tier`), so it never merges two DISTINCT edges - the safe superset is
/// preserved. `max`/`min` are the scalar SQLite functions, making the update order-independent so a
/// rebuild from the log re-derives byte-identical provenance regardless of fold order. This one
/// localized change dedups every fold arm at once, without touching a single call site.
#[allow(clippy::too_many_arguments)]
fn add_edge(
    tx: &Transaction,
    from: &str,
    to: &str,
    rel: &str,
    at: i64,
    src: Position,
    project: &str,
    tier: &str,
) -> Result<(), Error> {
    let updated = tx
        .execute(
            "UPDATE edges SET source = max(source, ?5), valid_from = min(valid_from, ?4)
             WHERE from_id = ?1 AND to_id = ?2 AND rel = ?3 AND tier = ?7 AND project = ?6
               AND valid_to IS NULL",
            params![from, to, rel, at, src as i64, project, tier],
        )
        .map_err(be)?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)",
            params![from, to, rel, at, src as i64, project, tier],
        )
        .map_err(be)?;
    }
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
    fn prune_drops_the_named_nodes_and_their_edges_and_a_replay_does_not_resurrect_them() {
        // Spec 21, unit 2: the single graph-mutation authority `rigger reset --runs` uses. It
        // deletes the named decision/finding nodes plus EVERY edge touching them, leaves the
        // rest (including any lesson, which the caller never puts in the drop set, and the
        // shared file itself), and - because the `applied` position ledger is untouched - a
        // replay of a pruned node's own event never resurrects it (drop from the graph WITHOUT
        // wiping the store).
        let p = Projector::open(":memory:", "test").unwrap();
        apply_decision(&p, 1, "keep-d", "keep", &["shared.rs"], "");
        apply_decision(&p, 2, "drop-d", "drop", &["shared.rs"], "");
        // A finding to drop, about the same file, raised by a reviewer (so it has a RAISED
        // edge INTO it and an ABOUT edge OUT of it - both must be swept).
        let finding = serde_json::json!({
            "id": "drop-f", "by": "lens", "summary": "x", "about": ["shared.rs"],
        });
        let mut fe = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&finding).unwrap());
        fe.position = 3;
        p.apply(&fe).unwrap();
        // A lesson about the same file: the caller never drops a lesson, so it must survive.
        let lesson =
            serde_json::json!({"id": "keep-lesson", "summary": "y", "about": ["shared.rs"]});
        let mut le = Event::new(TYPE_LESSON_LEARNED, serde_json::to_vec(&lesson).unwrap());
        le.position = 4;
        p.apply(&le).unwrap();

        // Before: every node is reachable from the shared file.
        let before = p.subgraph(&["shared.rs".to_string()], 2).unwrap();
        for id in ["keep-d", "drop-d", "drop-f", "keep-lesson"] {
            assert!(
                before.nodes.iter().any(|n| n.id == id),
                "{id} present before prune"
            );
        }

        let removed = p
            .prune(&["drop-d".to_string(), "drop-f".to_string()])
            .unwrap();
        assert_eq!(removed, 2, "exactly the two named nodes are removed");

        let after = p.subgraph(&["shared.rs".to_string()], 2).unwrap();
        for id in ["drop-d", "drop-f"] {
            assert!(
                !after.nodes.iter().any(|n| n.id == id),
                "{id} is pruned from the graph"
            );
            assert!(
                !after.edges.iter().any(|e| e.from == id || e.to == id),
                "every edge touching {id} is pruned"
            );
        }
        for id in ["keep-d", "keep-lesson", "shared.rs"] {
            assert!(
                after.nodes.iter().any(|n| n.id == id),
                "{id} is preserved (only the named nodes are pruned)"
            );
        }

        // A replay of the pruned decision's event (same position) does NOT resurrect it: the
        // position is still marked applied, so the fold is a no-op - the prune is durable.
        apply_decision(&p, 2, "drop-d", "drop", &["shared.rs"], "");
        let replayed = p.subgraph(&["shared.rs".to_string()], 2).unwrap();
        assert!(
            !replayed.nodes.iter().any(|n| n.id == "drop-d"),
            "a pruned node is not resurrected by a replay of its event"
        );
    }

    #[test]
    fn subgraph_finds_the_governing_decision() {
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
        apply_decision(&p, 1, "d1", "x", &["mod.rs"], "");
        apply_decision(&p, 1, "d1", "x", &["mod.rs"], ""); // same position, replayed
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let governs = g.edges.iter().filter(|e| e.rel == REL_GOVERNS).count();
        assert_eq!(governs, 1, "a replayed event must not double the edge");
    }

    /// Fold a `FileTouched` (`by` touches `path`) from its raw on-log JSON at `pos`, exactly the
    /// event the loop records each time an agent writes a file. `secs` sets the event's
    /// valid-from (when the touch happened) so a test can assert the collapsed edge keeps the
    /// EARLIEST assertion time; `pos` becomes the edge's `source`, so the LATEST assertion wins.
    fn apply_touch(p: &Projector, pos: u64, by: &str, path: &str, secs: u64) {
        let payload = serde_json::json!({ "path": path, "by": by });
        let mut e = Event::new(TYPE_FILE_TOUCHED, serde_json::to_vec(&payload).unwrap())
            .with_valid_from(UNIX_EPOCH + std::time::Duration::from_secs(secs));
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// Every LIVE `TOUCHES` edge as `(from, to, source, valid_from)`, read straight from the
    /// table (not through the live `subgraph` filter), so a test can COUNT the rows and prove a
    /// re-assertion collapsed into the one existing live edge rather than accreting a row per fold.
    fn live_touches(p: &Projector) -> Vec<(String, String, i64, i64)> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT from_id, to_id, source, valid_from FROM edges
                 WHERE rel = ?1 AND valid_to IS NULL ORDER BY from_id, to_id",
            )
            .unwrap();
        stmt.query_map(params![REL_TOUCHES], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    #[test]
    fn touches_folds_to_one_live_edge_with_latest_provenance() {
        // Spec 40 criterion 1: every `FileTouched` re-asserts `agent --TOUCHES--> file`, and the
        // old bare-insert fold appended a fresh live row per touch (measured: 45 identical live
        // rows for a single relationship - 60% of the live graph redundant). The upsert-live fold
        // collapses a re-assertion into the ONE existing live edge, bumping its provenance to the
        // LATEST assertion (source) and keeping the EARLIEST valid_from - so N touches yield
        // exactly ONE live edge, while a DIFFERENT agent or a DIFFERENT file still folds its own
        // distinct live edge (dedup removes only EXACT (from, rel, to, tier) duplicates).
        let p = Projector::open(":memory:", "test").unwrap();

        // agent-a touches src/f.rs four times (positions 10..=13; valid_from 100..=400s).
        apply_touch(&p, 10, "agent-a", "src/f.rs", 100);
        apply_touch(&p, 11, "agent-a", "src/f.rs", 200);
        apply_touch(&p, 12, "agent-a", "src/f.rs", 300);
        apply_touch(&p, 13, "agent-a", "src/f.rs", 400);

        // A DIFFERENT agent and a DIFFERENT file each fold their own distinct live edge.
        apply_touch(&p, 14, "agent-b", "src/f.rs", 500);
        apply_touch(&p, 15, "agent-a", "src/g.rs", 600);

        let f = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(100));
        let g = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(600));
        let b = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(500));
        assert_eq!(
            live_touches(&p),
            vec![
                // a->f collapsed from FOUR folds to ONE: source = latest (13), valid_from = earliest.
                ("agent-a".to_string(), "src/f.rs".to_string(), 13, f),
                // a different FILE is a distinct edge, untouched by the a->f dedup.
                ("agent-a".to_string(), "src/g.rs".to_string(), 15, g),
                // a different AGENT is a distinct edge, untouched by the a->f dedup.
                ("agent-b".to_string(), "src/f.rs".to_string(), 14, b),
            ],
            "N touches of one relationship collapse to ONE live edge (source=latest, valid_from=earliest); a different agent/file keeps its own distinct live edge"
        );
    }

    #[test]
    fn a_re_assertion_after_invalidation_folds_a_new_live_edge_dedup_keys_on_live_edges_only() {
        // Spec 40 criterion 2: the upsert-live fold keys on LIVE edges ONLY - `add_edge`'s dedup
        // UPDATE carries `AND valid_to IS NULL`. So it collapses a re-assertion into an EXISTING
        // live edge, but NEVER suppresses a legitimate re-assertion of a relationship that has since
        // been INVALIDATED. Drive the GOVERNS supersession path: d1 governs mod.rs (live), d2
        // supersedes d1 (stamping valid_to on d1's GOVERNS edge - invalidated, not deleted), then d1
        // is re-asserted. Because the one prior d1->mod.rs GOVERNS edge is invalidated, the dedup
        // UPDATE matches no live row and the fold INSERTs a NEW live edge, retained beside the dead
        // one. Were the dedup keyed on ALL edges instead of live-only, the re-assertion would be
        // swallowed into the invalidated row and no live edge would exist - the relationship would
        // be silently lost.
        //
        // The structural re-extraction variant of live-only scoping (a superseded
        // CONTAINS/REFERENCES edge re-asserted by a fresh batch) is already exercised by
        // `re_extraction_supersedes_a_files_prior_structural_edges_without_deleting_them`; the
        // GOVERNS supersession path here is the demonstration unique to this criterion. Scope is
        // strictly criterion 2 (live-only scoping): it does NOT own the TOUCHES re-assert fold
        // (criterion 1) nor the rebuild-collapse of pre-existing duplicates (criterion 3).
        let p = Projector::open(":memory:", "test").unwrap();

        // d1 governs mod.rs, then d2 supersedes d1 - stamping valid_to on d1's GOVERNS edge.
        apply_decision(&p, 1, "d1", "v1", &["mod.rs"], "");
        apply_decision(&p, 2, "d2", "v2", &[], "d1");

        // Precondition: exactly ONE d1->mod.rs GOVERNS edge exists and it is now INVALIDATED
        // (valid_to set), so NO live d1->mod.rs GOVERNS edge remains for the dedup to key on.
        let after_supersede: Vec<_> = edges_from(&p, "d1")
            .into_iter()
            .filter(|t| t.1 == REL_GOVERNS && t.0 == "mod.rs")
            .collect();
        assert_eq!(
            after_supersede.len(),
            1,
            "precondition: one d1->mod.rs GOVERNS edge after supersession; got {after_supersede:?}"
        );
        assert!(
            after_supersede[0].2.is_some(),
            "precondition: the supersession invalidated d1's GOVERNS edge (valid_to set); got {after_supersede:?}"
        );
        assert!(
            !p.subgraph(&["mod.rs".to_string()], 2)
                .unwrap()
                .edges
                .iter()
                .any(|e| e.rel == REL_GOVERNS && e.from == "d1"),
            "precondition: the invalidated edge is absent from the live view before the re-assertion"
        );

        // d1 is re-asserted at a later position - a legitimate re-assertion after invalidation.
        apply_decision(&p, 3, "d1", "v1", &["mod.rs"], "");

        // Live-only scoping: the dedup did NOT collapse the re-assertion into the dead row. A NEW
        // live edge is folded and RETAINED beside the invalidated one - exactly ONE historical +
        // ONE live d1->mod.rs GOVERNS edge.
        let after_reassert: Vec<_> = edges_from(&p, "d1")
            .into_iter()
            .filter(|t| t.1 == REL_GOVERNS && t.0 == "mod.rs")
            .collect();
        assert_eq!(
            after_reassert.len(),
            2,
            "the re-assertion folds a NEW row beside the invalidated one, not swallowed into it; got {after_reassert:?}"
        );
        assert_eq!(
            after_reassert.iter().filter(|t| t.2.is_none()).count(),
            1,
            "exactly ONE d1->mod.rs GOVERNS edge is live after the re-assertion; got {after_reassert:?}"
        );
        assert_eq!(
            after_reassert.iter().filter(|t| t.2.is_some()).count(),
            1,
            "the prior invalidated edge is retained (valid_to stamped), never overwritten; got {after_reassert:?}"
        );

        // The live view a grounding consumer reads once again shows d1 governing mod.rs - the
        // re-assertion took effect through a fresh live edge, not the suppressed dead one.
        assert!(
            p.subgraph(&["mod.rs".to_string()], 2)
                .unwrap()
                .edges
                .iter()
                .any(|e| e.rel == REL_GOVERNS && e.from == "d1" && e.to == "mod.rs"),
            "the re-asserted d1->mod.rs GOVERNS edge is LIVE in the projection"
        );
    }

    #[test]
    fn a_rebuild_collapses_existing_duplicate_live_edges_to_one_per_relationship() {
        // Spec 40 criterion 3 (rebuild-dedup / projection idempotency). The graph is a rebuildable
        // projection of the log (spec 29a), so the operational cleanup for the measured 39,340
        // duplicate live edges is a fresh graph REBUILD. This proves it: a log that under the OLD
        // bare `INSERT ... valid_to = NULL` accreted K identical live edges per relationship, folded
        // from scratch into a FRESH projection with the upsert-live `add_edge`, yields exactly ONE
        // live edge per `(from, rel, to, tier)` - with distinct relationships each surviving as
        // their own single live edge. This owns the rebuild-dedup; it leans on (but does not own)
        // the upsert-live fold arm (criterion 1) or the live-only scoping (criterion 2).

        // The canonical log the rebuild re-folds: agent-a --TOUCHES--> src/f.rs re-asserted 45
        // times (the measured worst case - one `FileTouched` fold per touch), interleaved with two
        // DISTINCT relationships (a different agent, a different file) that must each survive the
        // rebuild as their own single live edge.
        let fold_log = |p: &Projector| {
            for pos in 1..=45u64 {
                apply_touch(p, pos, "agent-a", "src/f.rs", 100 * pos);
            }
            apply_touch(p, 46, "agent-b", "src/f.rs", 5000);
            apply_touch(p, 47, "agent-a", "src/g.rs", 6000);
        };

        // PREMISE - reproduce the dirty on-disk graph the OLD bare-insert left behind. Each of the
        // 45 folds ran exactly this `INSERT ... valid_to = NULL`, so the pre-rebuild graph.db
        // carried 45 live rows for the ONE relationship (identical from/rel/to/tier; only
        // source/valid_from differ). Seed that state directly so the rebuild has a real duplicate
        // pile to collapse, not a hypothetical one.
        let dirty = Projector::open(":memory:", "test").unwrap();
        {
            let conn = dirty.conn.lock().unwrap();
            for pos in 1..=45i64 {
                conn.execute(
                    "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
                     VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)",
                    params![
                        "agent-a",
                        "src/f.rs",
                        REL_TOUCHES,
                        to_nanos(UNIX_EPOCH + std::time::Duration::from_secs((100 * pos) as u64)),
                        pos,
                        "test",
                        TIER_EXTRACTED
                    ],
                )
                .unwrap();
            }
        }
        assert_eq!(
            live_touches(&dirty).len(),
            45,
            "premise: the old bare-insert fold left K=45 identical live rows for one relationship - the duplicates a rebuild must collapse"
        );

        // REBUILD - discard the dirty graph and fold the SAME log from scratch into a FRESH, EMPTY
        // projection. Every relationship collapses to exactly ONE live edge: the 45-fold agent-a
        // ->src/f.rs to a single row (source = latest position 45, valid_from = earliest), and the
        // two DISTINCT relationships each to their own single live edge.
        let rebuilt = Projector::open(":memory:", "test").unwrap();
        fold_log(&rebuilt);

        let f = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(100));
        let g = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(6000));
        let b = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(5000));
        let want = vec![
            // agent-a->src/f.rs: 45 duplicate live edges collapsed to ONE (source=45, valid_from=earliest).
            ("agent-a".to_string(), "src/f.rs".to_string(), 45, f),
            // a different FILE is a distinct relationship, its own single live edge.
            ("agent-a".to_string(), "src/g.rs".to_string(), 47, g),
            // a different AGENT is a distinct relationship, its own single live edge.
            ("agent-b".to_string(), "src/f.rs".to_string(), 46, b),
        ];
        assert_eq!(
            live_touches(&rebuilt),
            want,
            "a rebuild collapses the 45 duplicate live edges to exactly ONE per (from, rel, to, tier); distinct relationships each survive as their own single live edge"
        );

        // REBUILDABLE - a rebuild is a pure, reproducible function of the log: folding the SAME log
        // into ANOTHER fresh, empty projection re-derives the identical single-edge-per-key set.
        let rebuilt_again = Projector::open(":memory:", "test").unwrap();
        fold_log(&rebuilt_again);
        assert_eq!(
            live_touches(&rebuilt_again),
            want,
            "rebuilding the same log from scratch re-derives the identical deduped live edges"
        );
    }

    fn apply_code_entity(
        p: &Projector,
        pos: u64,
        file: &str,
        name: &str,
        kind: &str,
        line: u32,
        lang: &str,
    ) {
        let payload = serde_json::json!({
            "file": file, "name": name, "kind": kind, "line": line, "lang": lang,
        });
        let mut e = Event::new(
            TYPE_CODE_ENTITY_EXTRACTED,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = pos;
        p.apply(&e).unwrap();
    }

    fn apply_edge_inferred(p: &Projector, pos: u64, file: &str, name: &str, lang: &str) {
        let payload = serde_json::json!({ "file": file, "name": name, "lang": lang });
        let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
        e.position = pos;
        p.apply(&e).unwrap();
    }

    #[test]
    fn code_extraction_events_fold_into_a_file_container_entities_and_structural_edges() {
        // Criterion 1: a source file's extraction EMITS CodeEntityExtracted (one per definition)
        // and EdgeInferred (one per reference); the ALWAYS-compiled fold turns them into a file
        // container node, code-entity nodes, and structural edges - so code structure lives in the
        // event-sourced projection, not a mutable side index. This test constructs the events by
        // hand (no extraction dependency) so it proves the fold in BOTH feature lanes: the fold
        // must build and pass with the `symbols` extractor absent.
        let p = Projector::open(":memory:", "test").unwrap();
        // Definition `apply_damage` in combat.rs, and a reference to `clamp` from the same file.
        apply_code_entity(
            &p,
            1,
            "src/combat.rs",
            "apply_damage",
            "function",
            7,
            "rust",
        );
        apply_edge_inferred(&p, 2, "src/combat.rs", "clamp", "rust");

        let g = p.subgraph(&["src/combat.rs".to_string()], 2).unwrap();

        // The file container node exists, tagged KIND_FILE.
        let file = g
            .nodes
            .iter()
            .find(|n| n.id == "src/combat.rs")
            .expect("file container node folded from the code events");
        assert_eq!(file.kind, KIND_FILE);

        // The definition folded into a code-entity node carrying its name, kind, and 1-based line.
        let ent = g
            .nodes
            .iter()
            .find(|n| n.id == "src/combat.rs::apply_damage")
            .expect("code-entity node folded from CodeEntityExtracted");
        assert_eq!(ent.kind, KIND_CODE_ENTITY);
        assert_eq!(
            ent.attrs.get("name").map(String::as_str),
            Some("apply_damage")
        );
        assert_eq!(ent.attrs.get("kind").map(String::as_str), Some("function"));
        assert_eq!(ent.attrs.get("line").map(String::as_str), Some("7"));

        // The file CONTAINS its definition (a structural edge from the container to the entity).
        assert!(
            g.edges.iter().any(|e| e.rel == REL_CONTAINS
                && e.from == "src/combat.rs"
                && e.to == "src/combat.rs::apply_damage"),
            "a CONTAINS edge ties the file to its definition; got {:?}",
            g.edges
        );
        // The file REFERENCES the symbol named at the reference site (a structural edge folded
        // from EdgeInferred), targeting the same file-scoped code-entity id.
        assert!(
            g.edges.iter().any(|e| e.rel == REL_REFERENCES
                && e.from == "src/combat.rs"
                && e.to == "src/combat.rs::clamp"),
            "a REFERENCES edge ties the file to the referenced symbol; got {:?}",
            g.edges
        );
    }

    /// Fold a design-intent concept (`kind` node `id`, titled `title`, from `doc`) from its raw
    /// on-log JSON at `pos`. Built by hand so the fold is exercised with the design-intent
    /// extraction pass absent - the always-compiled arm must run in BOTH feature lanes.
    fn apply_doc_concept(p: &Projector, pos: u64, kind: &str, id: &str, title: &str, doc: &str) {
        let payload = serde_json::json!({ "kind": kind, "id": id, "title": title, "doc": doc });
        let mut e = Event::new(
            TYPE_DOC_CONCEPT_EXTRACTED,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = pos;
        p.apply(&e).unwrap();
    }

    #[test]
    fn design_intent_concept_events_fold_into_the_four_design_intent_node_kinds() {
        // Criterion 1 (spec 29b): a design-intent extraction pass EMITS DocConceptExtracted events
        // (one per concept) that the ALWAYS-compiled fold turns into design-doc / arch-decision /
        // handbook-rule / rationale nodes - so the design-intent layer lives in the event-sourced
        // projection alongside the code half. Built by hand here (no extraction dependency) so it
        // proves the fold in BOTH feature lanes: the fold arm and the four node kinds must build and
        // pass with the design-intent extractor absent (the light lane).
        let p = Projector::open(":memory:", "test").unwrap();
        // A reference-architecture doc, a load-bearing decision, a spec-shape rule, and a WHY
        // comment - the four design-intent sources the criterion names, each its own node kind.
        apply_doc_concept(
            &p,
            1,
            KIND_DESIGN_DOC,
            "docs/architecture.md",
            "Reference architecture",
            "docs/architecture.md",
        );
        apply_doc_concept(
            &p,
            2,
            KIND_ARCH_DECISION,
            "docs/adr/0001-code-as-events.md",
            "Code structure is ingested as events",
            "docs/adr/0001-code-as-events.md",
        );
        apply_doc_concept(
            &p,
            3,
            KIND_HANDBOOK_RULE,
            "docs/handbook.md#one-owner-per-criterion",
            "Each criterion names its sole owner",
            "docs/handbook.md",
        );
        apply_doc_concept(
            &p,
            4,
            KIND_RATIONALE,
            "src/combat.rs#L7",
            "WHY: clamp keeps damage non-negative",
            "src/combat.rs",
        );

        let g = p
            .subgraph(
                &[
                    "docs/architecture.md".to_string(),
                    "docs/adr/0001-code-as-events.md".to_string(),
                    "docs/handbook.md#one-owner-per-criterion".to_string(),
                    "src/combat.rs#L7".to_string(),
                ],
                1,
            )
            .unwrap();
        let kind_of = |id: &str| g.nodes.iter().find(|n| n.id == id).map(|n| n.kind.as_str());
        assert_eq!(
            kind_of("docs/architecture.md"),
            Some(KIND_DESIGN_DOC),
            "a reference-architecture doc folds into a design-doc node; got {:?}",
            g.nodes
        );
        assert_eq!(
            kind_of("docs/adr/0001-code-as-events.md"),
            Some(KIND_ARCH_DECISION),
            "a load-bearing decision folds into an arch-decision node; got {:?}",
            g.nodes
        );
        assert_eq!(
            kind_of("docs/handbook.md#one-owner-per-criterion"),
            Some(KIND_HANDBOOK_RULE),
            "a spec-shape rule folds into a handbook-rule node; got {:?}",
            g.nodes
        );
        assert_eq!(
            kind_of("src/combat.rs#L7"),
            Some(KIND_RATIONALE),
            "a WHY comment folds into a rationale node; got {:?}",
            g.nodes
        );

        // The concept's title and source doc ride onto the node's attrs (provenance a later
        // criterion's design-intent edges key their links off).
        let ra = g
            .nodes
            .iter()
            .find(|n| n.id == "docs/architecture.md")
            .expect("the design-doc node folded");
        assert_eq!(
            ra.attrs.get("title").map(String::as_str),
            Some("Reference architecture")
        );
        assert_eq!(
            ra.attrs.get("doc").map(String::as_str),
            Some("docs/architecture.md")
        );
    }

    /// Fold a design-intent link (`from --rel--> to`) from its raw on-log JSON at `pos`. Built by
    /// hand so the fold is exercised with the design-intent extraction pass absent - the
    /// always-compiled arm must run in BOTH feature lanes.
    fn apply_doc_link(p: &Projector, pos: u64, from: &str, rel: &str, to: &str) {
        let payload = serde_json::json!({ "from": from, "to": to, "rel": rel });
        let mut e = Event::new(
            TYPE_DOC_LINK_EXTRACTED,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = pos;
        p.apply(&e).unwrap();
    }

    #[test]
    fn design_intent_link_events_fold_into_the_five_design_intent_edges() {
        // Criterion 2 (spec 29b): a design-intent extraction pass EMITS DocLinkExtracted events
        // (one per link) that the ALWAYS-compiled fold turns into the five typed design-intent
        // edges - design-doc --SPECIFIES--> code, arch-decision --CONSTRAINS--> code, handbook-rule
        // --GOVERNS--> code (reusing REL_GOVERNS), rationale --explains--> code, and design-doc
        // --references--> doc - so the design-intent layer's links live in the event-sourced
        // projection alongside the code half. Built by hand here (no extraction dependency) so it
        // proves the fold in BOTH feature lanes: the fold arm and the edge relations must build and
        // pass with the design-intent extractor absent (the light lane).
        let p = Projector::open(":memory:", "test").unwrap();
        // Fold the four design-intent source nodes first (criterion 1), so each edge emanates from a
        // real design-intent node of the right kind - the from-side identity the criterion names.
        apply_doc_concept(
            &p,
            1,
            KIND_DESIGN_DOC,
            "docs/architecture.md",
            "RA",
            "docs/architecture.md",
        );
        apply_doc_concept(
            &p,
            2,
            KIND_ARCH_DECISION,
            "docs/adr/0001-code-as-events.md",
            "Code as events",
            "docs/adr/0001-code-as-events.md",
        );
        apply_doc_concept(
            &p,
            3,
            KIND_HANDBOOK_RULE,
            "docs/handbook.md",
            "Rules",
            "docs/handbook.md",
        );
        apply_doc_concept(
            &p,
            4,
            KIND_RATIONALE,
            "src/combat.rs#L7",
            "WHY: clamp",
            "src/combat.rs",
        );

        // The five links the criterion names, one per relation.
        apply_doc_link(
            &p,
            5,
            "docs/architecture.md",
            REL_SPECIFIES,
            "src/contextgraph/sqlite.rs",
        );
        apply_doc_link(
            &p,
            6,
            "docs/adr/0001-code-as-events.md",
            REL_CONSTRAINS,
            "src/conductor.rs",
        );
        apply_doc_link(&p, 7, "docs/handbook.md", REL_GOVERNS, "src/spawn.rs");
        apply_doc_link(&p, 8, "src/combat.rs#L7", REL_EXPLAINS, "src/combat.rs");
        apply_doc_link(
            &p,
            9,
            "docs/architecture.md",
            REL_DOC_REFERENCES,
            "docs/addendum.md",
        );

        let g = p
            .subgraph(
                &[
                    "docs/architecture.md".to_string(),
                    "docs/adr/0001-code-as-events.md".to_string(),
                    "docs/handbook.md".to_string(),
                    "src/combat.rs#L7".to_string(),
                ],
                1,
            )
            .unwrap();
        let has_edge = |from: &str, rel: &str, to: &str| {
            g.edges
                .iter()
                .any(|e| e.from == from && e.rel == rel && e.to == to && e.tier == TIER_EXTRACTED)
        };
        assert!(
            has_edge(
                "docs/architecture.md",
                REL_SPECIFIES,
                "src/contextgraph/sqlite.rs"
            ),
            "a design-doc SPECIFIES the code it designs; got {:?}",
            g.edges
        );
        assert!(
            has_edge(
                "docs/adr/0001-code-as-events.md",
                REL_CONSTRAINS,
                "src/conductor.rs"
            ),
            "an arch-decision CONSTRAINS the code it binds; got {:?}",
            g.edges
        );
        assert!(
            has_edge("docs/handbook.md", REL_GOVERNS, "src/spawn.rs"),
            "a handbook-rule GOVERNS the file it rules (reusing REL_GOVERNS); got {:?}",
            g.edges
        );
        assert!(
            has_edge("src/combat.rs#L7", REL_EXPLAINS, "src/combat.rs"),
            "a rationale explains the code it annotates; got {:?}",
            g.edges
        );
        assert!(
            has_edge(
                "docs/architecture.md",
                REL_DOC_REFERENCES,
                "docs/addendum.md"
            ),
            "a design-doc references the doc it cites; got {:?}",
            g.edges
        );
    }

    #[test]
    fn a_doc_link_with_an_unrecognized_rel_folds_nothing_and_never_errors() {
        // The fold matches only the five design-intent relations; a payload carrying any other
        // relation string folds nothing (defensive - the emit only ever produces the five), and
        // never errors the rebuild. Mirrors the concept arm's unrecognized-kind guard.
        let p = Projector::open(":memory:", "test").unwrap();
        apply_doc_concept(
            &p,
            1,
            KIND_DESIGN_DOC,
            "docs/architecture.md",
            "RA",
            "docs/architecture.md",
        );
        apply_doc_link(
            &p,
            2,
            "docs/architecture.md",
            "TELEPORTS",
            "src/contextgraph/sqlite.rs",
        );
        let g = p
            .subgraph(&["docs/architecture.md".to_string()], 1)
            .unwrap();
        assert!(
            g.edges.is_empty(),
            "an unrecognized design-intent relation folds no edge; got {:?}",
            g.edges
        );
    }

    #[test]
    fn a_governed_doc_path_promotes_to_a_design_doc_node_in_both_fold_orders() {
        // One-graph identity (spec 29b, addendum 6.1 single id space): an architecture doc is folded
        // as a bare KIND_ARTIFACT the moment a decision GOVERNS it - which happens in a real run
        // (the decision stream cites the addenda by path). When that SAME path is later ingested as
        // design intent, it must become a design-doc node, not stay a bare artifact, or the
        // design-doc query would MISS the reference architecture - defeating the spec's core goal.
        // The promotion is order-independent: the specific kind wins whichever event folds first.
        let doc = "docs/architecture-addendum-context-management.md";

        // Order A: governed-first (artifact), then ingested (design-doc) -> promotes to design-doc.
        let a = Projector::open(":memory:", "test").unwrap();
        apply_decision(&a, 1, "d-ctx-mgmt", "context management RA", &[doc], "");
        apply_doc_concept(&a, 2, KIND_DESIGN_DOC, doc, "Context management", doc);
        let g = a.subgraph(&[doc.to_string()], 1).unwrap();
        let n = g.nodes.iter().find(|n| n.id == doc).expect("node folded");
        assert_eq!(
            n.kind, KIND_DESIGN_DOC,
            "a governed artifact PROMOTES to design-doc when ingested; got {:?}",
            n
        );
        assert_eq!(
            n.attrs.get("title").map(String::as_str),
            Some("Context management"),
            "the ingested title rides onto the promoted node"
        );

        // Order B: ingested-first (design-doc), then governed (artifact) -> stays design-doc (a
        // later bare-artifact reference never DEMOTES the established specific kind).
        let b = Projector::open(":memory:", "test").unwrap();
        apply_doc_concept(&b, 1, KIND_DESIGN_DOC, doc, "Context management", doc);
        apply_decision(&b, 2, "d-ctx-mgmt", "context management RA", &[doc], "");
        let g = b.subgraph(&[doc.to_string()], 1).unwrap();
        let n = g.nodes.iter().find(|n| n.id == doc).expect("node folded");
        assert_eq!(
            n.kind, KIND_DESIGN_DOC,
            "a later governing reference never DEMOTES the design-doc node; got {:?}",
            n
        );
    }

    /// Fold a code DEFINITION (`file` defines `name` at `line`) from its raw on-log JSON at `pos`.
    /// `fresh` marks the FIRST event of an extraction batch: the fold supersedes the file's prior
    /// structural edges before folding this one, so a re-extraction replaces rather than accretes.
    fn apply_batch_def(p: &Projector, pos: u64, file: &str, name: &str, line: u32, fresh: bool) {
        let payload = serde_json::json!({
            "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
            "fresh": fresh,
        });
        let mut e = Event::new(
            TYPE_CODE_ENTITY_EXTRACTED,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// Fold a code REFERENCE (`file` references `name`) from its raw on-log JSON at `pos`. `fresh`
    /// marks the first event of an extraction batch, exactly as for [`apply_batch_def`].
    fn apply_batch_ref(p: &Projector, pos: u64, file: &str, name: &str, fresh: bool) {
        let payload =
            serde_json::json!({ "file": file, "name": name, "lang": "rust", "fresh": fresh });
        let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// Fold a CALLER-ATTRIBUTED reference (spec 37): `file` references `name` from inside the
    /// enclosing definition `caller`, exactly the event the emit pass produces for a call in a
    /// function body. Mirrors [`apply_batch_ref`] but sets the `caller` field the c3 fold reads.
    fn apply_batch_ref_caller(p: &Projector, pos: u64, file: &str, name: &str, caller: &str) {
        let payload = serde_json::json!({
            "file": file, "name": name, "lang": "rust", "caller": caller,
        });
        let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// Every edge from `from`, as `(to, rel, valid_to)`, read STRAIGHT from the table - INCLUDING
    /// invalidated rows (a set `valid_to`) that the live `subgraph` filter hides. This is what lets
    /// a test prove supersede-not-delete: a superseded edge is RETAINED with `valid_to` stamped, so
    /// a historical / as-of reader still reaches it, not removed.
    fn edges_from(p: &Projector, from: &str) -> Vec<(String, String, Option<i64>)> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT to_id, rel, valid_to FROM edges
                 WHERE from_id = ?1 ORDER BY rel, to_id, valid_from",
            )
            .unwrap();
        stmt.query_map([from], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    #[test]
    fn re_extraction_supersedes_a_files_prior_structural_edges_without_deleting_them() {
        // Criterion 3: re-extracting a CHANGED file SUPERSEDES rather than overwrites. The FIRST
        // event of each extraction batch carries `fresh`, so the fold sets `valid_to` on the file's
        // prior live structural edges (CONTAINS / REFERENCES) before folding the new batch. The live
        // `subgraph` at the new position then shows the new entities and NONE of the removed ones,
        // while every old edge stays in the table with `valid_to` stamped - a historical / as-of
        // query still reaches the previous graph (supersede-not-delete, spec 29a section 6.4).
        let p = Projector::open(":memory:", "test").unwrap();
        let file = "src/a.rs";

        // Initial extraction: defs `foo` (line 5) and `bar` (line 9), plus a reference to `helper`.
        apply_batch_def(&p, 1, file, "foo", 5, true); // first event of the batch: fresh
        apply_batch_def(&p, 2, file, "bar", 9, false);
        apply_batch_ref(&p, 3, file, "helper", false);

        // Precondition: the initial graph holds both definitions and the reference, all live.
        let g0 = p.subgraph(&[file.to_string()], 2).unwrap();
        assert!(
            g0.edges
                .iter()
                .any(|e| e.rel == REL_CONTAINS && e.to == "src/a.rs::bar"),
            "precondition: bar is CONTAINed before re-extraction; got {:?}",
            g0.edges
        );
        assert!(
            g0.edges
                .iter()
                .any(|e| e.rel == REL_REFERENCES && e.to == "src/a.rs::helper"),
            "precondition: helper is REFERENCEd before re-extraction; got {:?}",
            g0.edges
        );

        // The file CHANGES and is re-extracted: `foo` moved to line 12, `bar` was DELETED, the
        // `helper` reference is gone, and a new reference to `other` appears.
        apply_batch_def(&p, 10, file, "foo", 12, true); // first event of the RE-extraction batch: fresh
        apply_batch_ref(&p, 11, file, "other", false);

        // LIVE view at the new position: foo is still contained (re-folded at its new line), the new
        // reference is live, and the removed bar / helper are GONE from the live subgraph.
        let g1 = p.subgraph(&[file.to_string()], 2).unwrap();
        assert!(
            g1.edges
                .iter()
                .any(|e| e.rel == REL_CONTAINS && e.to == "src/a.rs::foo"),
            "foo is still CONTAINed after re-extraction; got {:?}",
            g1.edges
        );
        assert!(
            g1.edges
                .iter()
                .any(|e| e.rel == REL_REFERENCES && e.to == "src/a.rs::other"),
            "the new `other` reference is live; got {:?}",
            g1.edges
        );
        assert!(
            !g1.edges.iter().any(|e| e.to == "src/a.rs::bar"),
            "the DELETED bar has no live edge after re-extraction; got {:?}",
            g1.edges
        );
        assert!(
            !g1.edges.iter().any(|e| e.to == "src/a.rs::helper"),
            "the removed helper reference is gone from the live view; got {:?}",
            g1.edges
        );
        // The surviving entity re-folded to its new line (the node upserts in place; only edges are
        // bi-temporal, so the entity id is stable and its attrs reflect the latest extraction).
        let foo = g1
            .nodes
            .iter()
            .find(|n| n.id == "src/a.rs::foo")
            .expect("foo entity present after re-extraction");
        assert_eq!(
            foo.attrs.get("line").map(String::as_str),
            Some("12"),
            "foo re-folded at its new line"
        );

        // Supersede-NOT-delete, read straight from the edge table (the live filter hides
        // invalidated rows): every prior structural edge is RETAINED with `valid_to` stamped, so a
        // historical / as-of query still reaches the old graph, and exactly one CONTAINS(foo) lives.
        let from_file = edges_from(&p, file);
        let contains_foo: Vec<_> = from_file
            .iter()
            .filter(|t| t.1 == REL_CONTAINS && t.0 == "src/a.rs::foo")
            .collect();
        assert_eq!(
            contains_foo.len(),
            2,
            "CONTAINS(foo) has one historical + one live row (nothing deleted); got {contains_foo:?}"
        );
        assert_eq!(
            contains_foo.iter().filter(|t| t.2.is_none()).count(),
            1,
            "exactly one CONTAINS(foo) is live; got {contains_foo:?}"
        );
        assert_eq!(
            contains_foo.iter().filter(|t| t.2.is_some()).count(),
            1,
            "the prior CONTAINS(foo) is retained with valid_to stamped; got {contains_foo:?}"
        );
        // The deleted bar's and removed helper's old edges are RETAINED but invalidated (their
        // valid_to is set), never deleted - so a historical query still sees the old file.
        let bar_edge = from_file
            .iter()
            .find(|t| t.1 == REL_CONTAINS && t.0 == "src/a.rs::bar")
            .expect("bar's CONTAINS edge is retained, not deleted");
        assert!(
            bar_edge.2.is_some(),
            "bar's CONTAINS edge is invalidated (valid_to set), not live; got {bar_edge:?}"
        );
        let helper_edge = from_file
            .iter()
            .find(|t| t.1 == REL_REFERENCES && t.0 == "src/a.rs::helper")
            .expect("helper's REFERENCES edge is retained, not deleted");
        assert!(
            helper_edge.2.is_some(),
            "helper's REFERENCES edge is invalidated (valid_to set), not live; got {helper_edge:?}"
        );
    }

    #[test]
    fn the_fold_adds_a_caller_attributed_calls_edge_alongside_the_references_edge() {
        // Spec 37 criterion 3: folding an `EdgeInferred` whose `caller` is `F` for a reference to
        // `G` in `<file>` adds a `<file>::F --CALLS--> <callee-of-G>` edge, WHILE the existing
        // `<file> --REFERENCES--> <callee-of-G>` edge is STILL produced. The CALLS edge is purely
        // additive and uses the SAME callee resolution the REFERENCES edge already uses.
        let p = Projector::open(":memory:", "test").unwrap();
        let file = "src/a.rs";

        // A file defining caller `F` and callee `G`, with `G` called from inside `F`'s body.
        apply_batch_def(&p, 1, file, "F", 1, true);
        apply_batch_def(&p, 2, file, "G", 5, false);
        apply_batch_ref_caller(&p, 3, file, "G", "F");

        let g = p.subgraph(&[file.to_string()], 2).unwrap();

        // The additive REFERENCES edge is UNCHANGED: the file still references G.
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_REFERENCES && e.from == file && e.to == "src/a.rs::G"),
            "the file-level REFERENCES(G) edge is still produced (additive); got {:?}",
            g.edges
        );
        // The new caller-attributed CALLS edge: F calls G, keyed by the enclosing definition.
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_CALLS && e.from == "src/a.rs::F" && e.to == "src/a.rs::G"),
            "the caller-attributed <file>::F --CALLS--> <file>::G edge is folded; got {:?}",
            g.edges
        );
        // Same callee resolution: the CALLS edge lands on the SAME same-file definition entity the
        // REFERENCES edge resolves to (both at the EXTRACTED tier for a resolved local symbol).
        let calls = g
            .edges
            .iter()
            .find(|e| e.rel == REL_CALLS && e.from == "src/a.rs::F")
            .expect("CALLS edge present");
        assert_eq!(
            calls.tier, TIER_EXTRACTED,
            "a resolved same-file call folds at EXTRACTED, mirroring its REFERENCES sibling; got {calls:?}"
        );
    }

    #[test]
    fn a_caller_less_reference_folds_no_calls_edge() {
        // Spec 37 (purely additive): a reference OUTSIDE every definition (a top-level `use`/import)
        // carries no caller, so it folds EXACTLY today's file-level REFERENCES edge and NO CALLS
        // edge. This pins the additive boundary: the CALLS edge appears ONLY when a caller is set.
        let p = Projector::open(":memory:", "test").unwrap();
        let file = "src/a.rs";

        apply_batch_def(&p, 1, file, "G", 5, true);
        apply_batch_ref(&p, 2, file, "G", false); // caller-less: a top-level reference

        let g = p.subgraph(&[file.to_string()], 2).unwrap();
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_REFERENCES && e.to == "src/a.rs::G"),
            "the caller-less reference still folds today's REFERENCES(G) edge; got {:?}",
            g.edges
        );
        assert!(
            !g.edges.iter().any(|e| e.rel == REL_CALLS),
            "a caller-less reference folds NO CALLS edge; got {:?}",
            g.edges
        );
    }

    #[test]
    fn re_extraction_supersedes_a_files_prior_calls_edges() {
        // Spec 37 + spec 29a criterion 3: a re-extracted file SUPERSEDES its own CALLS edges under
        // the same `fresh` batch boundary as its CONTAINS/REFERENCES edges. A CALLS edge's `from_id`
        // is `<file>::<caller>` (not the bare file node), so the supersede must retire it too -
        // otherwise a changed file would ACCRETE stale call edges rather than replace them.
        let p = Projector::open(":memory:", "test").unwrap();
        let file = "src/a.rs";

        // Initial extraction: F calls G.
        apply_batch_def(&p, 1, file, "F", 1, true);
        apply_batch_def(&p, 2, file, "G", 5, false);
        apply_batch_ref_caller(&p, 3, file, "G", "F");
        let g0 = p.subgraph(&[file.to_string()], 2).unwrap();
        assert!(
            g0.edges
                .iter()
                .any(|e| e.rel == REL_CALLS && e.from == "src/a.rs::F"),
            "precondition: F --CALLS--> G is live before re-extraction; got {:?}",
            g0.edges
        );

        // The file CHANGES: F no longer calls anything; the call is GONE.
        apply_batch_def(&p, 10, file, "F", 1, true); // fresh: first event of the re-extraction batch

        // LIVE view: the stale CALLS edge is GONE from the live subgraph (superseded, not accreted).
        let g1 = p.subgraph(&[file.to_string()], 2).unwrap();
        assert!(
            !g1.edges.iter().any(|e| e.rel == REL_CALLS),
            "the removed call is superseded - no live CALLS edge after re-extraction; got {:?}",
            g1.edges
        );
        // Supersede-NOT-delete: the old CALLS row is RETAINED with `valid_to` stamped (bi-temporal).
        let from_caller = edges_from(&p, "src/a.rs::F");
        let calls_row = from_caller
            .iter()
            .find(|t| t.1 == REL_CALLS && t.0 == "src/a.rs::G")
            .expect("the prior F --CALLS--> G row is retained, not deleted");
        assert!(
            calls_row.2.is_some(),
            "the prior CALLS edge is invalidated (valid_to set), not live; got {calls_row:?}"
        );
    }

    #[test]
    fn a_cross_file_calls_edge_upgrades_ambiguous_to_inferred_with_its_references_twin() {
        // Spec 37 tier-consistency: a CALLS edge to a callee defined in ANOTHER file, folded BEFORE
        // that definition exists, is tiered AMBIGUOUS - then the definition's convergent upgrade
        // promotes it AMBIGUOUS -> INFERRED, identically to its REFERENCES twin, so the CALLS edge
        // never lags its sibling's confidence. One tier authority governs both structural edges.
        let p = Projector::open(":memory:", "test").unwrap();
        let a = "src/a.rs";
        let b = "src/b.rs";

        // File A: `F` calls `G`, but `G` is NOT yet defined anywhere the graph knows -> AMBIGUOUS.
        apply_batch_def(&p, 1, a, "F", 1, true);
        apply_batch_ref_caller(&p, 2, a, "G", "F");
        let g_pre = p.subgraph(&[a.to_string()], 2).unwrap();
        let calls_pre = g_pre
            .edges
            .iter()
            .find(|e| e.rel == REL_CALLS && e.from == "src/a.rs::F")
            .expect("CALLS edge present pre-definition");
        assert_eq!(
            calls_pre.tier, TIER_AMBIGUOUS,
            "a call to a not-yet-known name folds AMBIGUOUS; got {calls_pre:?}"
        );

        // File B DEFINES `G`: the convergent upgrade promotes A's cross-file edges to INFERRED.
        apply_batch_def(&p, 3, b, "G", 9, true);

        let g_post = p.subgraph(&[a.to_string()], 2).unwrap();
        let calls_post = g_post
            .edges
            .iter()
            .find(|e| e.rel == REL_CALLS && e.from == "src/a.rs::F")
            .expect("CALLS edge present post-definition");
        assert_eq!(
            calls_post.tier, TIER_INFERRED,
            "the cross-file CALLS edge upgrades AMBIGUOUS -> INFERRED with its REFERENCES twin; got {calls_post:?}"
        );
        let refs_post = g_post
            .edges
            .iter()
            .find(|e| e.rel == REL_REFERENCES && e.to == "src/a.rs::G")
            .expect("REFERENCES twin present");
        assert_eq!(
            refs_post.tier, TIER_INFERRED,
            "the REFERENCES twin also upgraded (baseline the CALLS edge must match); got {refs_post:?}"
        );
    }

    #[test]
    fn a_blast_radius_computed_event_folds_to_nothing_idempotently() {
        // spec 16 unit 3: BlastRadiusComputed is PURE AUDIT - the projector matches no fold arm
        // for it (it falls to the `_ => {}` sink), so it adds NO node and NO edge, and re-applying
        // the SAME position (a replay) stays a no-op. This is what lets the audit ride the shared
        // stream without perturbing the context graph the reviewers read.
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
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

    /// Emit a ReviewFinding in the EXACT shape production records (the spec 11 review
    /// protocol): `{id, by, summary, about}` on the data, plus the emitting spawn's id on
    /// `meta.spawn` (the key `rigger emit` and the MCP server stamp on every real emit) -
    /// and NOTHING on `data.unit`, because a real finding never carries one. So a discard
    /// fold that keyed on a `$.unit` attr would match nothing here, exactly as it matches
    /// nothing in production; the fold must key on a field production actually sets.
    fn apply_review_finding(
        p: &Projector,
        pos: u64,
        id: &str,
        by: &str,
        spawn: &str,
        about: &[&str],
    ) {
        let payload = serde_json::json!({
            "id": id, "by": by, "summary": "x", "about": about,
        });
        let mut e = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&payload).unwrap())
            .with_meta(crate::conductor::META_SPAWN, spawn);
        e.position = pos;
        p.apply(&e).unwrap();
    }

    #[test]
    fn an_explicitly_discarded_finding_is_invalidated_upheld_and_undisposed_stay_live() {
        // Spec 25, criterion 1 (disposition-expiry, the DISCARD trigger): a finding the
        // adjudicator NAMES in its verdict line's `discarded` array is DISCARDED - folding
        // that adjudicator SpawnResult sets valid_to on the finding's RAISED/ABOUT edges
        // (invalidate, never delete - mirroring the decision-supersession arm), so the live
        // subgraph filter (valid_to IS NULL) stops returning it. The findings are emitted in
        // the EXACT production shape (id/by/summary/about + meta.spawn, NO data.unit), so this
        // proves the fold fires against what production actually records, not a hand-injected
        // unit attr. A finding the verdict UPHELD stays live, and so does a finding named in
        // NEITHER upheld NOR discarded: the discard keys on the explicit `discarded` array,
        // never the complement of `upheld`, so a reject's own still-open motivating findings
        // survive for the remediation to see.
        let p = Projector::open(":memory:", "test").unwrap();
        apply_review_finding(&p, 1, "f-discard", "lens:tech", "u1/lens:tech#0", &["a.rs"]);
        apply_review_finding(&p, 2, "f-upheld", "lens:sdet", "u1/lens:sdet#0", &["a.rs"]);
        apply_review_finding(&p, 3, "f-open", "lens:tech", "u1/lens:tech#0", &["a.rs"]);

        // Before the verdict, every finding is reachable from the file it is ABOUT.
        let before = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        for id in ["f-discard", "f-upheld", "f-open"] {
            assert!(
                before.nodes.iter().any(|n| n.id == id),
                "{id} present before the verdict"
            );
        }

        // The adjudicator UPHOLDS f-upheld and explicitly DISCARDS f-discard; f-open is named
        // in neither - a reject's still-open motivating finding that must survive.
        let verdict = r#"{"verdict":"reject","upheld":["f-upheld"],"discarded":["f-discard"],"cause":"genuine-defect"}"#;
        let mut e = crate::spawn::SpawnResult::ok("u1/adjudicator#0", verdict)
            .to_event()
            .unwrap();
        e.position = 4;
        p.apply(&e).unwrap();

        let after = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        assert!(
            !after.nodes.iter().any(|n| n.id == "f-discard"),
            "the explicitly discarded finding is pruned from the live subgraph (its edges were invalidated)"
        );
        assert!(
            !after
                .edges
                .iter()
                .any(|x| x.from == "f-discard" || x.to == "f-discard"),
            "no live edge touches the discarded finding"
        );
        assert!(
            after.nodes.iter().any(|n| n.id == "f-upheld"),
            "the UPHELD finding stays live"
        );
        assert!(
            after.nodes.iter().any(|n| n.id == "f-open"),
            "a finding named in NEITHER upheld nor discarded stays live - the discard keys on \
             the explicit `discarded` array, never the complement of upheld"
        );
    }

    #[test]
    fn a_verdict_that_names_no_discarded_array_expires_nothing() {
        // The over-invalidation the discard MUST NOT do (spec 25 c1): the discard set is the
        // EXPLICIT `discarded` array, never the complement of `upheld`. An approve that
        // upholds one finding and names no `discarded` array expires NOTHING - every finding
        // the review raised, upheld or not, stays live. Real verdicts routinely omit `upheld`
        // entirely (56/234 adjudications approve with none), so a complement-of-upheld fold
        // would sweep a whole review here; keying on `discarded` cannot.
        let p = Projector::open(":memory:", "test").unwrap();
        apply_review_finding(&p, 1, "f-kept", "lens:tech", "u1/lens:tech#0", &["a.rs"]);
        apply_review_finding(
            &p,
            2,
            "f-also-kept",
            "lens:sdet",
            "u1/lens:sdet#0",
            &["a.rs"],
        );

        let verdict = r#"{"verdict":"approve","upheld":["f-kept"]}"#;
        let mut e = crate::spawn::SpawnResult::ok("u1/adjudicator#0", verdict)
            .to_event()
            .unwrap();
        e.position = 3;
        p.apply(&e).unwrap();

        let after = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        for id in ["f-kept", "f-also-kept"] {
            assert!(
                after.nodes.iter().any(|n| n.id == id),
                "{id} stays live - a verdict that names no `discarded` array expires nothing \
                 (the discard is never the complement of upheld)"
            );
        }
    }

    fn apply_adjudication(p: &Projector, pos: u64, spawn: &str, verdict: &str) {
        let mut e = crate::spawn::SpawnResult::ok(spawn, verdict)
            .to_event()
            .unwrap();
        e.position = pos;
        p.apply(&e).unwrap();
    }

    fn apply_unit_integrated(p: &Projector, pos: u64, unit: &str, commit: &str) {
        // Production shape: the conductor emits `{"id": <unit>, "commit": ...}` at every
        // UNIT_INTEGRATED site (the `id` key, NOT `unit`). Building it this way proves the
        // fold parses what production actually records, not a hand-tuned `unit` payload.
        let payload = serde_json::json!({"id": unit, "commit": commit});
        let mut e = Event::new(TYPE_UNIT_INTEGRATED, serde_json::to_vec(&payload).unwrap());
        e.position = pos;
        p.apply(&e).unwrap();
    }

    #[test]
    fn an_upheld_finding_expires_when_its_unit_integrates_but_stays_live_until_then() {
        // Spec 25, criterion 2 (disposition-expiry, the UPHELD-AND-ADDRESSED trigger): a
        // finding the adjudicator UPHELD is RESOLVED once the unit that owns it INTEGRATES
        // (addresses it). The adjudicator's SpawnResult MARKS each upheld finding-of-unit
        // (disposition=upheld + the unit it belongs to) without invalidating; folding that
        // unit's TYPE_UNIT_INTEGRATED then sets valid_to on the marked finding's RAISED/ABOUT
        // edges, so the live subgraph filter (valid_to IS NULL) stops returning it. Two guards
        // this proves: an upheld finding whose unit has NOT integrated stays live (marking
        // alone expires nothing), and the trigger fires against the EXACT production payload -
        // the conductor emits UNIT_INTEGRATED with an `id` key, and the findings carry NO
        // `data.unit` (only the adjudicator spawn id names the unit), so a fold keyed on a
        // hand-injected `unit` attr would be vacuous here.
        let p = Projector::open(":memory:", "test").unwrap();
        // f-a is upheld for unit u1 (which will integrate); f-b is upheld for unit u2 (which
        // will NOT integrate). Emitted in production shape: id/by/summary/about + meta.spawn,
        // no data.unit.
        apply_review_finding(&p, 1, "f-a", "lens:sdet", "u1/lens:sdet#0", &["a.rs"]);
        apply_review_finding(&p, 2, "f-b", "lens:arch", "u2/lens:arch#0", &["b.rs"]);

        // Each unit's adjudicator UPHOLDS its finding. This MARKS the finding (records the
        // disposition and its owning unit, taken from the adjudicator spawn id) but must NOT
        // invalidate anything yet - the finding is not addressed until its unit integrates.
        apply_adjudication(
            &p,
            3,
            "u1/adjudicator#0",
            r#"{"verdict":"approve","upheld":["f-a"]}"#,
        );
        apply_adjudication(
            &p,
            4,
            "u2/adjudicator#0",
            r#"{"verdict":"approve","upheld":["f-b"]}"#,
        );

        // Marking alone expires nothing: both upheld findings stay live until their unit lands.
        let marked = p
            .subgraph(&["a.rs".to_string(), "b.rs".to_string()], 2)
            .unwrap();
        for id in ["f-a", "f-b"] {
            assert!(
                marked.nodes.iter().any(|n| n.id == id),
                "{id} is upheld but stays live until its unit integrates"
            );
        }

        // u1 integrates: its upheld finding f-a is now ADDRESSED and expires.
        apply_unit_integrated(&p, 5, "u1", "commitsha");

        let after = p
            .subgraph(&["a.rs".to_string(), "b.rs".to_string()], 2)
            .unwrap();
        assert!(
            !after.nodes.iter().any(|n| n.id == "f-a"),
            "the upheld finding of the INTEGRATED unit is pruned (its edges were invalidated)"
        );
        assert!(
            !after.edges.iter().any(|x| x.from == "f-a" || x.to == "f-a"),
            "no live edge touches the addressed finding"
        );
        assert!(
            after.nodes.iter().any(|n| n.id == "f-b"),
            "an upheld finding whose unit has NOT integrated stays live"
        );
    }

    #[test]
    fn a_discard_under_run_a_never_suppresses_the_same_finding_re_raised_under_a_later_run_b() {
        // Spec 25, criterion 3 (disposition-expiry, RUN-SCOPING - the DISCARD trigger): expiry
        // is by DISPOSITION, not by run age. A finding DISCARDED under run A has its RAISED /
        // ABOUT edges invalidated (valid_to set), so it drops from the live subgraph - but
        // re-raising the SAME finding id under a LATER run B must return it LIVE again. This
        // holds by fold-position semantics, not a stored run label: invalidate_finding_edges
        // only touches edges that currently hold (valid_to IS NULL) AT FOLD TIME, and a re-raise
        // add_edges a FRESH row (valid_to NULL) that run A's already-folded discard never saw.
        // So run A's invalidation is scoped to run A's own edges and can never suppress a run B
        // re-raise. This criterion OWNS that run-scoping guarantee; it does NOT own the discard
        // trigger (criterion 1 does) - the discard here is only the disposition-under-A
        // precondition.
        let p = Projector::open(":memory:", "test").unwrap();

        // Run A raises f-x about a.rs; it is live.
        apply_review_finding(&p, 1, "f-x", "lens:tech", "u1/lens:tech#0", &["a.rs"]);
        assert!(
            p.subgraph(&["a.rs".to_string()], 2)
                .unwrap()
                .nodes
                .iter()
                .any(|n| n.id == "f-x"),
            "f-x is live after run A raises it"
        );

        // Run A's adjudicator DISCARDS f-x: its edges are invalidated, so it drops from the live
        // subgraph. (Criterion 1's trigger, used here only to set the run-A disposition.)
        apply_adjudication(
            &p,
            2,
            "u1/adjudicator#0",
            r#"{"verdict":"reject","discarded":["f-x"]}"#,
        );
        assert!(
            !p.subgraph(&["a.rs".to_string()], 2)
                .unwrap()
                .nodes
                .iter()
                .any(|n| n.id == "f-x"),
            "run A's discard invalidates f-x's edges, so it drops from the live subgraph"
        );

        // A LATER run B re-raises the SAME finding id. The re-raise appends FRESH valid_to-NULL
        // RAISED / ABOUT edges that run A's earlier discard never touched, so f-x is LIVE again -
        // run A's disposition never suppresses a run B re-raise (expiry by disposition, not run
        // age).
        apply_review_finding(&p, 3, "f-x", "lens:tech", "u1-run-b/lens:tech#0", &["a.rs"]);
        let after_b = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        assert!(
            after_b.nodes.iter().any(|n| n.id == "f-x"),
            "the SAME finding re-raised under a later run B is returned LIVE by subgraph - run A's \
             discard never suppresses a B re-raise"
        );
        assert!(
            after_b
                .edges
                .iter()
                .any(|x| x.from == "f-x" || x.to == "f-x"),
            "run B's re-raise created a fresh live edge for f-x (run A's invalidation stayed \
             scoped to run A's edges)"
        );
    }

    #[test]
    fn an_upheld_mark_never_expires_the_same_finding_re_raised_before_its_unit_integrates() {
        // Spec 25, criterion 3 (disposition-expiry, RUN-SCOPING - the UPHELD-AND-ADDRESSED
        // trigger): a finding UPHELD for unit u1 under run A is MARKED (disposition=upheld,
        // unit=u1) and expires only when u1 INTEGRATES. If a LATER run B re-raises the SAME
        // finding between the mark and the integrate, that re-raise re-runs ensure_node, whose
        // ON CONFLICT COALESCE(excluded.attrs, nodes.attrs) overwrites the whole attrs and so
        // CLEARS the stale mark, and appends fresh valid_to-NULL edges. So when u1 integrates,
        // the run-B re-raised finding no longer matches the marked-for-u1 SELECT and stays LIVE,
        // while a sibling still-marked finding (never re-raised) is correctly expired. This
        // proves run A's upheld disposition never over-invalidates a run B re-raise (the
        // cross-run over-invalidation guard). This criterion OWNS that run-scoping guarantee; it
        // does NOT own the upheld-and-addressed trigger (criterion 2 does).
        let p = Projector::open(":memory:", "test").unwrap();

        // Run A raises two findings about a.rs, both upheld for u1: f-reraised (which run B will
        // re-raise) and f-control (which nothing re-raises), emitted in production shape.
        apply_review_finding(
            &p,
            1,
            "f-reraised",
            "lens:sdet",
            "u1/lens:sdet#0",
            &["a.rs"],
        );
        apply_review_finding(&p, 2, "f-control", "lens:arch", "u1/lens:arch#0", &["a.rs"]);
        apply_adjudication(
            &p,
            3,
            "u1/adjudicator#0",
            r#"{"verdict":"approve","upheld":["f-control","f-reraised"]}"#,
        );

        // A LATER run B re-raises ONLY f-reraised. The re-raise COALESCE-overwrites its attrs,
        // clearing the disposition=upheld mark, and appends fresh live edges.
        apply_review_finding(
            &p,
            4,
            "f-reraised",
            "lens:sdet",
            "u1-run-b/lens:sdet#0",
            &["a.rs"],
        );

        // u1 integrates: it expires only the findings STILL marked upheld-for-u1. f-control is
        // still marked and expires; f-reraised's mark was cleared by run B's re-raise, so it is
        // untouched and stays LIVE.
        apply_unit_integrated(&p, 5, "u1", "commitsha");

        let after = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        assert!(
            after.nodes.iter().any(|n| n.id == "f-reraised"),
            "the finding re-raised under a later run B stays LIVE when u1 integrates - the \
             re-raise cleared the stale upheld mark, so run A's disposition never over-invalidates \
             a B re-raise"
        );
        assert!(
            after
                .edges
                .iter()
                .any(|x| x.from == "f-reraised" || x.to == "f-reraised"),
            "run B's re-raise left f-reraised with a fresh live edge"
        );
        assert!(
            !after.nodes.iter().any(|n| n.id == "f-control"),
            "the sibling upheld finding that was NEVER re-raised is correctly expired on integrate \
             - proving the integrate genuinely fires and f-reraised's survival is the run-scoping \
             effect, not a vacuous no-op"
        );
    }

    #[test]
    fn unit_started_creates_assigned_to_and_blocks() {
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
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
        let p = Projector::open(":memory:", "test").unwrap();
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

    /// The project stamped on every node row, as `(id, project)` pairs in `(id, project)`
    /// order. Spec 28 criterion 1's write tag lives on the raw `project` column; `subgraph`
    /// does not yet filter by it (that read filter is criterion 2), so this reads the column
    /// directly to prove the fold stamps it. Reads through the same connection, so on a shared
    /// backend it observes every project's committed rows.
    fn node_projects(p: &Projector) -> Vec<(String, String)> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, project FROM nodes ORDER BY id, project")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// The project stamped on every edge row, in insertion order. Same rationale as
    /// [`node_projects`]: the write tag is on the raw column, read directly.
    fn edge_projects(p: &Projector) -> Vec<String> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT project FROM edges ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn every_node_and_edge_carries_the_projects_scope_on_fold() {
        // Spec 28, criterion 1 (the WRITE tag): a fold of the same events under project P tags
        // EVERY resulting node and edge with P, derived from the SAME plain project string
        // `Namespaced::new` uses for the `proj-<id>-` stream prefix - threaded in through the
        // scoped `Projector::open(path, project)` constructor. This is net-new node/edge state:
        // there was no project field before, so the whole value is the tag appearing on every
        // row of a fold.
        //
        // Proven three ways: (1) under project "alpha" every node/edge row reads project=alpha;
        // (2) the SAME events folded under "beta" tag "beta", so the scope is derived from the
        // constructor, not hard-coded; (3) on ONE SHARED backend the SAME seed id d1 coexists as
        // two distinct rows (one per project) - the composite (id, project) key that makes the
        // tag genuinely isolating state, which read-isolation (c2) and rebuild-under-scope (c4)
        // rely on. On a shared backend the two projects' events occupy DISTINCT global positions
        // (the `Namespaced` decorator scopes streams over one global log), so beta folds the
        // same-shaped event at a later position - the `applied` ledger never mistakes it for an
        // already-folded event.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        // A decision governing shared.rs, decided by agent-7: folds a decision node, an artifact
        // node (shared.rs), an agent node (agent-7), a DECIDED edge and a GOVERNS edge - every
        // node kind and both edge directions from one event.
        let fold_at = |p: &Projector, pos: u64| {
            let payload = serde_json::json!({
                "id": "d1", "summary": "x", "governs": ["shared.rs"], "supersedes": "",
            });
            let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
            e.position = pos;
            e.meta.insert(META_ACTOR.to_string(), "agent-7".to_string());
            p.apply(&e).unwrap();
        };

        // (1) project "alpha": every row tagged "alpha".
        let alpha = Projector::open(shared, "alpha").unwrap();
        fold_at(&alpha, 1);
        let a_nodes = node_projects(&alpha);
        assert!(
            a_nodes.iter().any(|(id, _)| id == "d1"),
            "the decision node was folded"
        );
        assert!(
            a_nodes.iter().all(|(_, proj)| proj == "alpha"),
            "every node carries project=alpha on fold, got {a_nodes:?}"
        );
        let a_edges = edge_projects(&alpha);
        assert!(!a_edges.is_empty(), "the fold produced edges");
        assert!(
            a_edges.iter().all(|proj| proj == "alpha"),
            "every edge carries project=alpha on fold, got {a_edges:?}"
        );

        // (2) same backend, project "beta": the SAME event (a later global position) tags "beta".
        let beta = Projector::open(shared, "beta").unwrap();
        fold_at(&beta, 2);
        let all_nodes = node_projects(&beta);
        // (3) the SAME seed id d1 now exists under BOTH projects - two distinct rows on ONE
        // shared backend (the composite (id, project) key), never one overwriting the other.
        let d1_projects: Vec<&str> = all_nodes
            .iter()
            .filter(|(id, _)| id == "d1")
            .map(|(_, proj)| proj.as_str())
            .collect();
        assert_eq!(
            d1_projects,
            vec!["alpha", "beta"],
            "the same seed id d1 coexists as one row per project on a shared backend"
        );
        // beta genuinely tagged beta, and alpha's row is untouched by beta's fold.
        assert!(
            all_nodes
                .iter()
                .any(|(id, proj)| id == "shared.rs" && proj == "beta"),
            "beta's fold of shared.rs is tagged beta, got {all_nodes:?}"
        );
        assert!(
            all_nodes
                .iter()
                .any(|(id, proj)| id == "shared.rs" && proj == "alpha"),
            "alpha's shared.rs row is untouched by beta's fold, got {all_nodes:?}"
        );
    }

    #[test]
    fn a_pre_project_graph_db_migrates_additively_backfilling_the_openers_identity() {
        // Spec 28 backward-compat (a GLOBAL CONSTRAINT criterion 1 owns as an ADDITIVE
        // migration): a graph.db written before the project scope existed has the OLD shape -
        // nodes(id PRIMARY KEY, kind, attrs) and edges(... source) with no `project` column.
        // Opening it with the scoped constructor migrates it in place WITHOUT wiping it:
        // existing rows survive, backfilled with the OPENER's identity (so a single-project
        // deployment reads identically once the criterion-2 filter lands), and the recreated
        // nodes table now carries the composite (id, project) key so a second project's
        // same-id fold coexists rather than overwriting.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();

        // Hand-build the OLD schema and seed a node + an edge the way pre-spec-28 code did.
        {
            let conn = Connection::open(path).unwrap();
            conn.execute_batch(
                "CREATE TABLE nodes (id TEXT PRIMARY KEY, kind TEXT NOT NULL, attrs TEXT);
                 CREATE TABLE edges (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
                   valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL
                 );
                 CREATE TABLE aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
                 CREATE TABLE applied (position INTEGER PRIMARY KEY);
                 INSERT INTO nodes (id, kind, attrs)
                   VALUES ('old-d', 'decision', '{\"summary\":\"legacy\"}');
                 INSERT INTO nodes (id, kind, attrs) VALUES ('old.rs', 'artifact', NULL);
                 INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source)
                   VALUES ('old-d', 'old.rs', 'GOVERNS', 100, NULL, 1);",
            )
            .unwrap();
        }

        // Open through the scoped constructor: the migration runs, backfilling the opener's id.
        let p = Projector::open(path, "legacy-proj").unwrap();

        // Existing rows survived AND are now tagged with the opener's identity.
        let nodes = node_projects(&p);
        assert!(
            nodes.contains(&("old-d".to_string(), "legacy-proj".to_string())),
            "the legacy decision node survived and is backfilled with the opener's identity, \
             got {nodes:?}"
        );
        assert!(
            nodes.contains(&("old.rs".to_string(), "legacy-proj".to_string())),
            "the legacy artifact node survived and is backfilled, got {nodes:?}"
        );
        assert_eq!(
            edge_projects(&p),
            vec!["legacy-proj".to_string()],
            "the legacy edge survived and is backfilled with the opener's identity"
        );
        // The migrated node's attrs are intact (the data was copied, not just the id).
        let g = p.subgraph(&["old.rs".to_string()], 2).unwrap();
        let d = g
            .nodes
            .iter()
            .find(|n| n.id == "old-d")
            .expect("the migrated decision is still reachable from the file it governs");
        assert_eq!(d.attrs.get("summary").map(String::as_str), Some("legacy"));

        // The recreated composite (id, project) key lets a DIFFERENT project fold the same id as
        // a distinct row - proving the migration produced the isolating schema, not just a
        // column. Reopening the same file re-runs the migration as a no-op (the column exists).
        let other = Projector::open(path, "other-proj").unwrap();
        let payload = serde_json::json!({"id": "old-d", "summary": "fresh", "governs": ["old.rs"]});
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = 2;
        other.apply(&e).unwrap();
        let after = node_projects(&other);
        let old_d: Vec<&str> = after
            .iter()
            .filter(|(id, _)| id == "old-d")
            .map(|(_, proj)| proj.as_str())
            .collect();
        assert_eq!(
            old_d,
            vec!["legacy-proj", "other-proj"],
            "the same id old-d coexists across the migrated project and a new one (composite PK)"
        );
    }

    /// The `(from_id, to_id, project)` of every edge touching node `id`, in `id` (insertion)
    /// order. Spec 28 criterion 3 reads the raw `edges` table directly - `subgraph`'s read
    /// filter is criterion 2, not yet relied on here - so on a shared backend it observes
    /// EVERY project's edges that reference `id`, exactly what a cross-project prune leak
    /// would show.
    fn edges_touching(p: &Projector, id: &str) -> Vec<(String, String, String)> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT from_id, to_id, project FROM edges
                 WHERE from_id = ?1 OR to_id = ?1 ORDER BY id",
            )
            .unwrap();
        stmt.query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    #[test]
    fn prune_is_project_scoped_leaving_another_projects_same_id_node_intact() {
        // Spec 28, criterion 3 (the PRUNE scope): `Projector::prune` is the single graph-mutation
        // authority `rigger reset --runs` uses to shed a dead run's nodes. On a SHARED backend it
        // must delete ONLY the pruning project's nodes and edges - pruning project P's dead-run
        // node must leave project Q's node with the SAME seed id fully intact. Without the scope
        // the id-keyed DELETE reaches across projects and wipes Q's row (and Q's edges) too, since
        // the composite (id, project) key (criterion 1) lets both projects hold that id at once.
        //
        // Fixture: ONE shared graph.db file, two projects (alpha, beta), each folding the SAME
        // decision "drop-d" (governing shared.rs, decided by agent-7) so the SAME seed id exists
        // as one row per project, WITH edges in both directions touching it (agent-7 -DECIDED->
        // drop-d and drop-d -GOVERNS-> shared.rs). Prune "drop-d" through ALPHA only.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        let fold_drop_d = |p: &Projector, pos: u64| {
            let payload = serde_json::json!({
                "id": "drop-d", "summary": "x", "governs": ["shared.rs"], "supersedes": "",
            });
            let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
            e.position = pos;
            e.meta.insert(META_ACTOR.to_string(), "agent-7".to_string());
            p.apply(&e).unwrap();
        };

        let alpha = Projector::open(shared, "alpha").unwrap();
        fold_drop_d(&alpha, 1);
        // beta folds the SAME event at a later global position (the Namespaced decorator scopes
        // streams over one global log), so beta's drop-d is a distinct row on the one backend.
        let beta = Projector::open(shared, "beta").unwrap();
        fold_drop_d(&beta, 2);

        // Before: the SAME seed id drop-d exists as one row per project, and BOTH projects have
        // edges touching it - so the survival assertions below are non-vacuous.
        let before = node_projects(&alpha);
        let drop_before: Vec<&str> = before
            .iter()
            .filter(|(id, _)| id == "drop-d")
            .map(|(_, proj)| proj.as_str())
            .collect();
        assert_eq!(
            drop_before,
            vec!["alpha", "beta"],
            "drop-d coexists under both projects on the shared backend before prune, got {before:?}"
        );
        let edges_before = edges_touching(&alpha, "drop-d");
        assert!(
            edges_before.iter().any(|(_, _, proj)| proj == "alpha"),
            "alpha has edges touching drop-d before prune, got {edges_before:?}"
        );
        assert!(
            edges_before.iter().any(|(_, _, proj)| proj == "beta"),
            "beta has edges touching drop-d before prune, got {edges_before:?}"
        );

        // Prune drop-d through ALPHA's projector.
        let removed = alpha.prune(&["drop-d".to_string()]).unwrap();
        assert_eq!(
            removed, 1,
            "prune removes EXACTLY alpha's one drop-d node, never reaching beta's same-id row"
        );

        // After: alpha's drop-d node is gone; beta's drop-d node (same id) is left fully intact.
        let after = node_projects(&alpha);
        assert!(
            !after
                .iter()
                .any(|(id, proj)| id == "drop-d" && proj == "alpha"),
            "alpha's drop-d node is pruned, got {after:?}"
        );
        assert!(
            after
                .iter()
                .any(|(id, proj)| id == "drop-d" && proj == "beta"),
            "beta's drop-d node with the SAME seed id is left fully intact, got {after:?}"
        );

        // Edges are project-scoped too: every alpha edge touching drop-d is swept, and every
        // beta edge touching drop-d survives - the prune never dangles or over-reaches across
        // projects.
        let edges_after = edges_touching(&alpha, "drop-d");
        assert!(
            !edges_after.iter().any(|(_, _, proj)| proj == "alpha"),
            "alpha's edges touching drop-d are all pruned, got {edges_after:?}"
        );
        assert!(
            !edges_after.is_empty() && edges_after.iter().all(|(_, _, proj)| proj == "beta"),
            "every surviving edge touching drop-d is beta's - beta's edges are untouched, \
             got {edges_after:?}"
        );
    }

    /// Fold a decision `id` (summary `summary`) governing `governs`, DECIDED by `actor`, at
    /// global `pos`. Used by the read-isolation test to seed two projects on ONE shared backend
    /// with the SAME seed ids but distinct project-scoped neighborhoods.
    fn apply_decision_by(
        p: &Projector,
        pos: u64,
        id: &str,
        summary: &str,
        governs: &[&str],
        actor: &str,
    ) {
        let payload = serde_json::json!({
            "id": id, "summary": summary, "governs": governs, "supersedes": "",
        });
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = pos;
        e.meta.insert(META_ACTOR.to_string(), actor.to_string());
        p.apply(&e).unwrap();
    }

    #[test]
    fn subgraph_isolates_reads_to_the_current_project_on_a_shared_backend() {
        // Spec 28, criterion 2 (the READ filter): one graph store holding TWO projects' folds
        // returns, via subgraph, ONLY the current project's nodes - even when both projects
        // contain a node with the SAME seed id. This mirrors, for the graph, what
        // `Namespaced::scope_filter` does for streams. The write tag (criterion 1) already puts
        // every row on a `(id, project)` key; this proves the read side never crosses the scope.
        //
        // One shared graph.db, two Projectors ("alpha", "beta"). BOTH fold a decision "d1" (and
        // an artifact "shared.rs") - the SAME seed ids under both projects - plus a
        // project-UNIQUE neighbor decision ("only-alpha" / "only-beta") governing the same file
        // and a project-unique DECIDING agent. On a shared backend the two projects occupy
        // DISTINCT global positions (the `Namespaced` decorator scopes streams over one global
        // log), so distinct positions keep the shared `applied` ledger from mistaking beta's
        // fold for an already-applied one.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        let alpha = Projector::open(shared, "alpha").unwrap();
        apply_decision_by(&alpha, 1, "d1", "alpha-d1", &["shared.rs"], "agent-alpha");
        apply_decision_by(&alpha, 2, "only-alpha", "x", &["shared.rs"], "agent-alpha");

        let beta = Projector::open(shared, "beta").unwrap();
        apply_decision_by(&beta, 3, "d1", "beta-d1", &["shared.rs"], "agent-beta");
        apply_decision_by(&beta, 4, "only-beta", "y", &["shared.rs"], "agent-beta");

        // alpha's read: seeded on the file BOTH projects share.
        let ag = alpha.subgraph(&["shared.rs".to_string()], 2).unwrap();
        // (1) same seed id d1 in two projects -> ONLY alpha's d1, exactly once.
        let d1s: Vec<&str> = ag
            .nodes
            .iter()
            .filter(|n| n.id == "d1")
            .map(|n| n.attrs.get("summary").map(String::as_str).unwrap_or(""))
            .collect();
        assert_eq!(
            d1s,
            vec!["alpha-d1"],
            "alpha's read returns ONLY alpha's d1 (never beta's), exactly one row, got {ag:?}"
        );
        // (2) exactly one shared.rs node (not beta's duplicate).
        assert_eq!(
            ag.nodes.iter().filter(|n| n.id == "shared.rs").count(),
            1,
            "the shared seed node appears once, scoped to alpha, got {ag:?}"
        );
        // (3) the traversal never crosses into beta's neighborhood.
        for leaked in ["only-beta", "agent-beta", "beta-d1"] {
            assert!(
                !ag.nodes.iter().any(|n| n.id == leaked),
                "alpha's read must not surface beta-only node {leaked}, got {ag:?}"
            );
            assert!(
                !ag.edges.iter().any(|e| e.from == leaked || e.to == leaked),
                "alpha's read must not surface any edge touching beta-only {leaked}, got {ag:?}"
            );
        }
        // alpha's own neighborhood is intact (the filter isolates, it does not empty the graph).
        for kept in ["only-alpha", "agent-alpha"] {
            assert!(
                ag.nodes.iter().any(|n| n.id == kept),
                "alpha's own node {kept} stays reachable under scope, got {ag:?}"
            );
        }

        // beta's read is the mirror image over the SAME shared backend.
        let bg = beta.subgraph(&["shared.rs".to_string()], 2).unwrap();
        let bd1s: Vec<&str> = bg
            .nodes
            .iter()
            .filter(|n| n.id == "d1")
            .map(|n| n.attrs.get("summary").map(String::as_str).unwrap_or(""))
            .collect();
        assert_eq!(
            bd1s,
            vec!["beta-d1"],
            "beta's read returns ONLY beta's d1, exactly one row, got {bg:?}"
        );
        for leaked in ["only-alpha", "agent-alpha", "alpha-d1"] {
            assert!(
                !bg.nodes.iter().any(|n| n.id == leaked),
                "beta's read must not surface alpha-only node {leaked}, got {bg:?}"
            );
        }
        for kept in ["only-beta", "agent-beta"] {
            assert!(
                bg.nodes.iter().any(|n| n.id == kept),
                "beta's own node {kept} stays reachable under scope, got {bg:?}"
            );
        }
    }

    #[test]
    fn resolve_is_project_scoped_on_a_shared_backend() {
        // Spec 28, criterion 2 (read isolation, the resolve read): `Projection::resolve`'s
        // node-existence fallback is a read of the nodes table, so on a shared backend it must
        // answer for the CURRENT project only - a node id that exists solely under project beta
        // must resolve to None for project alpha, never a cross-project false-positive. The
        // alias path is unaffected (the `aliases` table carries no project column and stays
        // shared); only the node-existence fallback is scoped.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        let alpha = Projector::open(shared, "alpha").unwrap();
        apply_decision(&alpha, 1, "a-only", "x", &["shared.rs"], "");
        let beta = Projector::open(shared, "beta").unwrap();
        apply_decision(&beta, 2, "b-only", "y", &["shared.rs"], "");

        // Each project resolves its OWN node id...
        assert_eq!(
            alpha.resolve("a-only").unwrap().as_deref(),
            Some("a-only"),
            "alpha resolves its own node"
        );
        assert_eq!(
            beta.resolve("b-only").unwrap().as_deref(),
            Some("b-only"),
            "beta resolves its own node"
        );
        // ...but NOT the other project's node, even though it exists on the shared backend.
        assert_eq!(
            alpha.resolve("b-only").unwrap(),
            None,
            "alpha must not resolve beta-only node (cross-project existence leak)"
        );
        assert_eq!(
            beta.resolve("a-only").unwrap(),
            None,
            "beta must not resolve alpha-only node (cross-project existence leak)"
        );
    }

    #[test]
    fn migrate_project_rekeys_rows_so_read_isolation_survives_identity_mint() {
        // Spec 28 GC5 (backward-compat): `Projector::migrate_project` is the graph analog of
        // `rename_stream_prefix` for the spec-09 identity mint. A single-project deployment folds
        // rows under its basename identity "oldname"; the mint renames its streams to a durable
        // identity, but the graph folds incrementally so the renamed streams are never re-folded
        // and the pre-mint rows keep the legacy scope. Once the read filter (criterion 2) scopes
        // reads to the minted identity, those rows orphan unless re-keyed. Re-keying moves ONLY
        // the named scope's rows, leaving another project's SAME-id rows on the shared backend
        // fully intact.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        // Pre-mint history under the legacy basename "oldname" (decision + its governed file).
        let legacy = Projector::open(shared, "oldname").unwrap();
        apply_decision(&legacy, 1, "pre-d", "s", &["pre.rs"], "");
        // An unrelated project "sibling" holds the SAME node id "pre-d" on the one shared backend.
        let sibling = Projector::open(shared, "sibling").unwrap();
        apply_decision(&sibling, 2, "pre-d", "sib", &["pre.rs"], "");

        // The minted projector re-keys ONLY the legacy scope's rows to the minted identity.
        let minted = Projector::open(shared, "mint123").unwrap();
        let moved = minted.migrate_project("oldname", "mint123").unwrap();
        assert_eq!(
            moved, 2,
            "both the pre-mint decision and its governed file re-key (2 nodes moved)"
        );

        // The minted read now returns the pre-mint history - it did NOT orphan.
        let g = minted.subgraph(&["pre.rs".to_string()], 2).unwrap();
        assert!(
            g.nodes.iter().any(|n| n.id == "pre-d"),
            "the re-keyed pre-mint decision is reachable under the minted identity, got {g:?}"
        );
        assert_eq!(
            minted.resolve("pre-d").unwrap().as_deref(),
            Some("pre-d"),
            "the pre-mint node resolves under the minted identity after migration"
        );

        // The legacy scope is now empty, and the sibling project's SAME-id rows are untouched.
        let projs = node_projects(&minted);
        assert!(
            !projs.iter().any(|(_, p)| p == "oldname"),
            "no row keeps the legacy scope after migration, got {projs:?}"
        );
        assert!(
            projs.iter().any(|(id, p)| id == "pre-d" && p == "sibling"),
            "another project's same-id row is left fully intact, got {projs:?}"
        );
        assert_eq!(
            projs
                .iter()
                .filter(|(id, p)| id == "pre-d" && p == "mint123")
                .count(),
            1,
            "exactly one minted pre-d row exists (the re-key never duplicated), got {projs:?}"
        );

        // Idempotent: a re-open after the migration re-keys nothing (the legacy scope is empty).
        assert_eq!(
            minted.migrate_project("oldname", "mint123").unwrap(),
            0,
            "re-keying again moves nothing"
        );
    }

    #[test]
    fn subgraph_traversal_and_edge_scope_are_pinned_independently_of_the_node_fetch() {
        // Spec 28 criterion 2, hardening. The base isolation test distinguishes projects by node
        // id, so the node-fetch scope alone could mask a traversal that wrongly crossed into
        // another project OR an edge fetched from another project. This fixture makes the
        // TRAVERSAL scope (`e.project` in the recursive CTE) and the EDGE-FETCH scope (`project`
        // on the edge read) each INDEPENDENTLY necessary, keyed on rows whose ids ALSO exist
        // under the reading project so the node-fetch scope cannot hide either leak.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        // In alpha, shared.rs connects to a-seed, and `bridge` exists but connects ONLY to
        // alpha-side.rs - there is NO alpha edge between shared.rs and bridge.
        let alpha = Projector::open(shared, "alpha").unwrap();
        apply_decision(&alpha, 1, "a-seed", "x", &["shared.rs"], "");
        apply_decision(&alpha, 2, "bridge", "x", &["alpha-side.rs"], "");

        // In beta the SAME ids exist, PLUS a beta edge between shared.rs and bridge (bridge
        // governs shared.rs) and a second beta edge between a-seed and shared.rs. Every endpoint
        // id also exists under alpha, so either edge would leak into alpha's read if a scope
        // clause were dropped - the node-fetch scope alone cannot hide it.
        let beta = Projector::open(shared, "beta").unwrap();
        apply_decision(&beta, 3, "a-seed", "y", &["shared.rs"], "");
        apply_decision(&beta, 4, "bridge", "y", &["shared.rs"], "");

        // Non-vacuous fixture: bridge exists under BOTH projects; beta has a shared.rs<->bridge
        // edge and alpha does not.
        let projs = node_projects(&alpha);
        assert!(
            projs.iter().any(|(id, p)| id == "bridge" && p == "alpha"),
            "bridge exists under alpha (so a leaked traversal would surface it), got {projs:?}"
        );
        assert!(
            projs.iter().any(|(id, p)| id == "bridge" && p == "beta"),
            "bridge exists under beta, got {projs:?}"
        );
        let bridge_edges = edges_touching(&alpha, "bridge");
        assert!(
            bridge_edges
                .iter()
                .any(|(f, t, p)| p == "beta" && (f == "shared.rs" || t == "shared.rs")),
            "beta has an edge between shared.rs and bridge (the traversal bait), got {bridge_edges:?}"
        );
        assert!(
            !bridge_edges
                .iter()
                .any(|(f, t, p)| p == "alpha" && (f == "shared.rs" || t == "shared.rs")),
            "alpha has NO edge between shared.rs and bridge, got {bridge_edges:?}"
        );

        let ag = alpha.subgraph(&["shared.rs".to_string()], 2).unwrap();
        // (1) TRAVERSAL scope: bridge is reachable from shared.rs ONLY via beta's edge, and its
        // id exists under alpha, so a dropped CTE `e.project` clause would surface alpha's bridge.
        assert!(
            !ag.nodes.iter().any(|n| n.id == "bridge"),
            "alpha's traversal must not cross beta's edge into `bridge`, got {ag:?}"
        );
        // (2) EDGE-FETCH scope: only alpha's own edge among the reached ids is returned. a-seed
        // and shared.rs both exist under beta with a beta edge between them, so a dropped edge
        // `project` clause would also return that beta edge. Exactly one alpha edge spans the set.
        assert_eq!(
            ag.edges.len(),
            1,
            "exactly alpha's one edge among the reached nodes is returned (never beta's), got {ag:?}"
        );
        assert!(
            ag.edges
                .iter()
                .all(|e| (e.from == "a-seed" && e.to == "shared.rs")
                    || (e.from == "shared.rs" && e.to == "a-seed")),
            "the returned edge is alpha's a-seed<->shared.rs edge, got {ag:?}"
        );
    }

    #[test]
    fn rebuilding_from_a_two_project_log_re_derives_two_correctly_scoped_subgraphs() {
        // Spec 28, criterion 4 (rebuild-under-scope). The graph is a REBUILDABLE projection of
        // the event log (addendum section 2.1), never hand-maintained state. Rebuilding it FROM
        // SCRATCH out of a single log that carries TWO projects' events re-derives two correctly-
        // scoped subgraphs - each project sees ONLY its own nodes - with NO MANUAL BACKFILL. The
        // project tag is re-DERIVED on every fold from the SAME injected identity that scopes the
        // streams, never stored as a mutable side fact a rebuild would drop. This owns rebuild-
        // under-scope; it leans on (but does not own) the write tag (criterion 1) and the read
        // filter (criterion 2).
        //
        // The shared global log interleaves both projects at DISTINCT positions (the `Namespaced`
        // decorator scopes each project's streams over one global log). A from-scratch rebuild
        // replays that whole log, routing each event to a Projector scoped to its OWNING project
        // against the SAME graph.db - exactly what re-deriving a shared-backend graph from
        // position 0 does after the graph.db is discarded. No prune / migrate / UPDATE is ever
        // called: the scope is a pure product of folding, which is what "no manual backfill"
        // means for a rebuild.

        // One canonical two-project log. Each entry is (owning project, event): the shape a
        // shared-backend replay sees - one global stream, every event attributable to its project
        // by its `Namespaced` prefix, at a DISTINCT global position. The seed ids "d1" and
        // "shared.rs" live in BOTH projects, so a rebuild that lost scope would MERGE them; a
        // correct rebuild keeps them apart.
        let two_project_log = || -> Vec<(&'static str, Event)> {
            let ev = |pos: u64, id: &str, summary: &str, actor: &str| -> Event {
                let payload = serde_json::json!({
                    "id": id, "summary": summary, "governs": ["shared.rs"], "supersedes": "",
                });
                let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
                e.position = pos;
                e.meta.insert(META_ACTOR.to_string(), actor.to_string());
                e
            };
            vec![
                ("alpha", ev(1, "d1", "alpha-d1", "agent-alpha")),
                ("beta", ev(2, "d1", "beta-d1", "agent-beta")),
                ("alpha", ev(3, "only-alpha", "x", "agent-alpha")),
                ("beta", ev(4, "only-beta", "y", "agent-beta")),
            ]
        };

        // Rebuild the WHOLE log into a fresh, EMPTY graph.db at `path`: replay every event,
        // folding each into a Projector scoped to its owning project (one scoped view per project
        // over the shared backend, kept in a `BTreeMap` so the rebuild is deterministic per the
        // Global constraint). A fresh db has no pre-existing rows, so nothing is ever backfilled -
        // the scope each row carries comes SOLELY from the fold. Returns, per project, the set of
        // node ids that project's scoped `subgraph` reaches from the file both share: the rebuilt,
        // scoped projection.
        let rebuild = |path: &str| -> BTreeMap<String, BTreeSet<String>> {
            let mut projectors: BTreeMap<String, Projector> = BTreeMap::new();
            for (proj, e) in two_project_log() {
                projectors
                    .entry(proj.to_string())
                    .or_insert_with(|| Projector::open(path, proj).unwrap())
                    .apply(&e)
                    .unwrap();
            }
            projectors
                .iter()
                .map(|(proj, p)| {
                    let g = p.subgraph(&["shared.rs".to_string()], 2).unwrap();
                    let ids = g
                        .nodes
                        .iter()
                        .map(|n| n.id.clone())
                        .collect::<BTreeSet<_>>();
                    (proj.clone(), ids)
                })
                .collect()
        };

        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("graph.db");
        let scoped = rebuild(first.to_str().unwrap());

        // Each project's rebuilt subgraph reaches EXACTLY its own nodes - never the other's, even
        // though both share the seed ids d1 and shared.rs. Exact-set equality pins BOTH failure
        // directions at once: no beta node leaks into alpha's rebuild (over-reach), and none of
        // alpha's own nodes go missing (under-derivation). agent-alpha/agent-beta are reached at
        // depth 2 (shared.rs <- d1/only-* via GOVERNS, then agent -> decision via DECIDED), so the
        // set exercises node, edge, and traversal re-derivation together.
        let want = |own_decision: &str, own_agent: &str| -> BTreeSet<String> {
            ["shared.rs", "d1", own_decision, own_agent]
                .iter()
                .map(|s| s.to_string())
                .collect()
        };
        assert_eq!(
            scoped.get("alpha"),
            Some(&want("only-alpha", "agent-alpha")),
            "the rebuilt alpha subgraph reaches EXACTLY alpha's own nodes (no beta leak, none \
             missing), got {scoped:?}"
        );
        assert_eq!(
            scoped.get("beta"),
            Some(&want("only-beta", "agent-beta")),
            "the rebuilt beta subgraph reaches EXACTLY beta's own nodes (no alpha leak, none \
             missing), got {scoped:?}"
        );
        // The shared seed d1 is re-derived under BOTH scopes as its OWN row (one per project),
        // never merged into one and never crossed over - the composite (id, project) key that
        // makes the projection isolating survives a from-scratch rebuild.
        assert!(
            scoped["alpha"].contains("d1") && scoped["beta"].contains("d1"),
            "the shared seed id d1 is re-derived under both project scopes, got {scoped:?}"
        );

        // REBUILDABLE: discard the graph.db and rebuild the SAME log from scratch into a DIFFERENT
        // fresh, empty db. The re-derived scoped projection is IDENTICAL - the scope is a pure,
        // reproducible function of the log, re-derived on every fold, not a mutable side fact a
        // rebuild would lose. (The first db is untouched by this second rebuild.)
        let dir2 = tempfile::tempdir().unwrap();
        let second = dir2.path().join("graph.db");
        let rebuilt_again = rebuild(second.to_str().unwrap());
        assert_eq!(
            rebuilt_again, scoped,
            "rebuilding the two-project log from scratch re-derives the identical scoped subgraphs"
        );
    }

    // ---- spec 29a criterion 2: the confidence tier on folded structural edges ----

    /// Fold a code definition event (`file` defines `name`) at `pos`.
    fn apply_def(p: &Projector, pos: u64, file: &str, name: &str) {
        let payload = serde_json::json!({
            "file": file, "name": name, "kind": "function", "line": 1, "lang": "rust",
        });
        let mut e = Event::new(
            TYPE_CODE_ENTITY_EXTRACTED,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// Fold a code reference event (`file` references `name`) at `pos`.
    fn apply_ref(p: &Projector, pos: u64, file: &str, name: &str) {
        let payload = serde_json::json!({ "file": file, "name": name, "lang": "rust" });
        let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// The tier of the one edge with relation `rel` landing on `to`, out of a subgraph.
    fn edge_tier(g: &Graph, rel: &str, to: &str) -> String {
        let matches: Vec<&Edge> = g
            .edges
            .iter()
            .filter(|e| e.rel == rel && e.to == to)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one {rel} edge to {to}; got {matches:?}"
        );
        matches[0].tier.clone()
    }

    #[test]
    fn every_structural_edge_carries_its_confidence_tier() {
        // Spec 29a criterion 2: every structural edge folds at a confidence tier - the precise/safe
        // split made a first-class edge attribute (addendum 6.2). One integrated graph exercises all
        // three tiers from real folded events:
        //   - a definition's CONTAINS edge, and a reference resolved to a SAME-file definition, fold
        //     EXTRACTED (explicit in source);
        //   - a reference whose name is defined in ANOTHER file folds INFERRED (derived / transitive);
        //   - a reference whose name is defined NOWHERE folds AMBIGUOUS (grep-visible-only).
        let p = Projector::open(":memory:", "test").unwrap();
        // combat.rs defines `apply_damage` and references it (same-file), references `shared` (a name
        // defined in util.rs), and references `magic` (defined nowhere).
        apply_def(&p, 1, "util.rs", "shared");
        apply_def(&p, 2, "combat.rs", "apply_damage");
        apply_ref(&p, 3, "combat.rs", "apply_damage");
        apply_ref(&p, 4, "combat.rs", "shared");
        apply_ref(&p, 5, "combat.rs", "magic");

        let g = p.subgraph(&["combat.rs".to_string()], 3).unwrap();

        // A definition's containment is the most explicit structural fact: EXTRACTED.
        assert_eq!(
            edge_tier(&g, REL_CONTAINS, "combat.rs::apply_damage"),
            TIER_EXTRACTED,
            "a CONTAINS edge folds at the EXTRACTED tier"
        );
        // A reference resolved to a same-file definition: EXTRACTED (explicit local reference).
        assert_eq!(
            edge_tier(&g, REL_REFERENCES, "combat.rs::apply_damage"),
            TIER_EXTRACTED,
            "a reference to a same-file definition folds EXTRACTED"
        );
        // A reference to a name defined in ANOTHER file: INFERRED (derived / transitive link).
        assert_eq!(
            edge_tier(&g, REL_REFERENCES, "combat.rs::shared"),
            TIER_INFERRED,
            "a reference to a name defined in another file folds INFERRED"
        );
        // A reference to a name defined nowhere known: AMBIGUOUS (grep-visible-only).
        assert_eq!(
            edge_tier(&g, REL_REFERENCES, "combat.rs::magic"),
            TIER_AMBIGUOUS,
            "a reference to a name defined nowhere folds AMBIGUOUS"
        );

        // Safe-superset invariant (addendum 2.4): tiering NEVER drops a reference. The three tiers
        // partition the reference edges, so their union recovers EVERY reference folded (3 here) -
        // the safe view EXTRACTED u INFERRED u AMBIGUOUS stays a superset of the grep union.
        let refs: Vec<&Edge> = g.edges.iter().filter(|e| e.rel == REL_REFERENCES).collect();
        assert_eq!(
            refs.len(),
            3,
            "every reference yields exactly one edge; got {refs:?}"
        );
        assert!(
            refs.iter().all(|e| e.tier == TIER_EXTRACTED
                || e.tier == TIER_INFERRED
                || e.tier == TIER_AMBIGUOUS),
            "every reference edge carries one of the three tiers; got {refs:?}"
        );
    }

    #[test]
    fn the_cross_file_inferred_tier_is_order_independent() {
        // The tier is a pure function of the FINAL log, not of fold interleaving (the convergence the
        // definition arm's AMBIGUOUS -> INFERRED upgrade guarantees, mirroring c1's kind promotion).
        // A cross-file reference lands INFERRED whether it folds AFTER its definition or BEFORE it.

        // Definition-first: util.rs defines `shared`, THEN combat.rs references it.
        let a = Projector::open(":memory:", "test").unwrap();
        apply_def(&a, 1, "util.rs", "shared");
        apply_ref(&a, 2, "combat.rs", "shared");
        let ga = a.subgraph(&["combat.rs".to_string()], 3).unwrap();
        assert_eq!(
            edge_tier(&ga, REL_REFERENCES, "combat.rs::shared"),
            TIER_INFERRED,
            "definition-first: the cross-file reference is INFERRED"
        );

        // Reference-first: combat.rs references `shared` while it is still unknown (folds AMBIGUOUS),
        // THEN util.rs defines it - the definition arm must upgrade the earlier reference to INFERRED.
        let b = Projector::open(":memory:", "test").unwrap();
        apply_ref(&b, 1, "combat.rs", "shared");
        // Before the definition folds, the reference is grep-visible-only: AMBIGUOUS.
        let mid = b.subgraph(&["combat.rs".to_string()], 3).unwrap();
        assert_eq!(
            edge_tier(&mid, REL_REFERENCES, "combat.rs::shared"),
            TIER_AMBIGUOUS,
            "reference-first, before any definition: the reference is AMBIGUOUS"
        );
        apply_def(&b, 2, "util.rs", "shared");
        let gb = b.subgraph(&["combat.rs".to_string()], 3).unwrap();
        assert_eq!(
            edge_tier(&gb, REL_REFERENCES, "combat.rs::shared"),
            TIER_INFERRED,
            "reference-first: the later definition promotes the earlier reference AMBIGUOUS -> INFERRED"
        );

        // Convergence: both fold orders reach the identical stored tier.
        assert_eq!(
            edge_tier(&ga, REL_REFERENCES, "combat.rs::shared"),
            edge_tier(&gb, REL_REFERENCES, "combat.rs::shared"),
            "the cross-file tier is order-independent"
        );
    }

    #[test]
    fn the_definition_upgrade_never_demotes_a_same_file_extracted_reference() {
        // The convergent upgrade targets only cross-file AMBIGUOUS references. A reference resolved
        // to a SAME-file definition is EXTRACTED and must stay EXTRACTED when the same name is later
        // defined in another file too - the upgrade excludes the definition's own entity id, and the
        // EXTRACTED reference is not AMBIGUOUS, so it is doubly protected from being pulled down.
        let p = Projector::open(":memory:", "test").unwrap();
        apply_def(&p, 1, "combat.rs", "shared");
        apply_ref(&p, 2, "combat.rs", "shared");
        apply_def(&p, 3, "util.rs", "shared");
        let g = p.subgraph(&["combat.rs".to_string()], 3).unwrap();
        assert_eq!(
            edge_tier(&g, REL_REFERENCES, "combat.rs::shared"),
            TIER_EXTRACTED,
            "a same-file reference stays EXTRACTED even after the name is also defined elsewhere"
        );
    }

    #[test]
    fn a_dev_loop_edge_folds_at_the_extracted_tier() {
        // Every non-code dev-loop edge (DECIDED / GOVERNS / ...) is an explicit fact on the log, so
        // it folds EXTRACTED (addendum 6.2) - the tier column is universal, not code-only.
        let p = Projector::open(":memory:", "test").unwrap();
        let mut e = Event::new(
            TYPE_DECISION_MADE,
            serde_json::to_vec(
                &serde_json::json!({"id": "d1", "summary": "x", "governs": ["combat.rs"]}),
            )
            .unwrap(),
        );
        e.position = 1;
        p.apply(&e).unwrap();
        let g = p.subgraph(&["d1".to_string()], 2).unwrap();
        assert_eq!(
            edge_tier(&g, REL_GOVERNS, "combat.rs"),
            TIER_EXTRACTED,
            "a GOVERNS dev-loop edge folds EXTRACTED"
        );
    }

    #[test]
    fn a_pre_tier_graph_db_migrates_additively_backfilling_the_extracted_tier() {
        // Additive backward-compat (spec 29a, addendum 6.2, "migrate in place like
        // migrate_project_scope"): a graph.db written before the tier existed has an `edges` table
        // with no `tier` column. Opening it must migrate in place WITHOUT wiping it, backfilling
        // every existing edge to EXTRACTED - the correct tier for a dev-loop edge - so the edge
        // survives and reads back tiered.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();

        // Hand-build a project-scoped-but-tier-less edges table and seed a GOVERNS edge.
        {
            let conn = Connection::open(path).unwrap();
            conn.execute_batch(
                "CREATE TABLE nodes (
                   id TEXT NOT NULL, kind TEXT NOT NULL, attrs TEXT,
                   project TEXT NOT NULL DEFAULT '', PRIMARY KEY (id, project)
                 );
                 CREATE TABLE edges (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
                   valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL,
                   project TEXT NOT NULL DEFAULT ''
                 );
                 CREATE TABLE aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
                 CREATE TABLE applied (position INTEGER PRIMARY KEY);
                 INSERT INTO nodes (id, kind, attrs, project)
                   VALUES ('old-d', 'decision', '{\"summary\":\"legacy\"}', 'p');
                 INSERT INTO nodes (id, kind, attrs, project) VALUES ('old.rs', 'artifact', NULL, 'p');
                 INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project)
                   VALUES ('old-d', 'old.rs', 'GOVERNS', 100, NULL, 1, 'p');",
            )
            .unwrap();
        }

        // Opening through the scoped constructor runs both migrations (project already present,
        // tier newly added) without error, and the legacy edge survives, backfilled to EXTRACTED.
        let p = Projector::open(path, "p").unwrap();
        let g = p.subgraph(&["old.rs".to_string()], 2).unwrap();
        assert_eq!(
            edge_tier(&g, REL_GOVERNS, "old.rs"),
            TIER_EXTRACTED,
            "the migrated legacy edge survives and backfills to the EXTRACTED tier"
        );
    }

    #[test]
    fn tier_default_matches_the_extracted_const() {
        // The `SCHEMA` / migration SQL hard-codes the tier column default as the literal 'extracted'
        // (a const cannot be spliced into the SQL literal). Pin that it stays in lockstep with
        // TIER_EXTRACTED, so a rename of the const can never silently diverge from the stored value.
        assert_eq!(TIER_EXTRACTED, "extracted");
    }

    // ---- spec 29a criterion 4: the code graph is REBUILDABLE from the log ----

    #[test]
    fn the_code_graph_is_rebuildable_from_the_log_re_deriving_identical_nodes_and_tiered_edges() {
        // Spec 29a criterion 4 (rebuild). The code graph is a REBUILDABLE projection of the event
        // log (the spec goal + the Global constraint "the code graph is a rebuildable projection"),
        // never a mutable side index. Discarding the graph.db and folding the SAME
        // CodeEntityExtracted / EdgeInferred log from scratch re-derives byte-identical
        // code-entity / file nodes and TIERED structural edges - so code structure survives purely
        // as a function of the log, with no mutable side artifact a rebuild could drop. This owns
        // rebuild; it leans on (but does not own) the extract-as-events fold (criterion 1), the
        // confidence tier (criterion 2), or supersede-on-re-extract (criterion 3).
        //
        // The canonical log exercises all three tiers AND the reverse (reference-before-definition)
        // fold order, so the rebuild has real teeth: the cross-file reference at position 1 folds
        // AMBIGUOUS (its name is unknown yet), then the definition at position 2 upgrades it to
        // INFERRED (criterion 2's convergent AMBIGUOUS -> INFERRED promotion). That upgrade is a
        // pure product of REPLAYING the whole log, not a cached side fact - so were the tier a
        // mutable artifact instead of a fold derivation, a from-scratch rebuild would lose it.
        // Every event carries fresh=false (a single extraction, not a re-extraction), so supersede
        // never fires: this criterion proves rebuild of the initial projection, not re-extraction.
        fn node_desc(n: &Node) -> String {
            // kind, id, and every derived attr (BTreeMap iterates key-sorted, so this is
            // deterministic) - a bare reference target has no attrs and reads back empty.
            let attrs = n
                .attrs
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("{} {} [{}]", n.kind, n.id, attrs)
        }
        fn edge_desc(e: &Edge) -> String {
            format!("{} -{}-> {} [{}]", e.from, e.rel, e.to, e.tier)
        }

        let fold_log = |p: &Projector| {
            apply_ref(p, 1, "combat.rs", "shared"); // cross-file ref, folds AMBIGUOUS first
            apply_def(p, 2, "util.rs", "shared"); // its definition promotes the ref to INFERRED
            apply_def(p, 3, "combat.rs", "apply_damage");
            apply_ref(p, 4, "combat.rs", "apply_damage"); // resolved to a same-file def -> EXTRACTED
            apply_ref(p, 5, "combat.rs", "magic"); // defined nowhere -> AMBIGUOUS
        };

        // Fold the whole log into a FRESH, EMPTY graph.db at `path` and read the code half of the
        // projection back as two order-independent sets: the code-entity / file NODES (id, kind,
        // and every derived attr) and the structural EDGES (from, rel, to, and the folded tier). A
        // fresh db has no pre-existing rows, so nothing is ever backfilled - what comes back is
        // purely what the fold derived. BTreeSets make the snapshot deterministic per the Global
        // constraint.
        let rebuild = |path: &str| -> (BTreeSet<String>, BTreeSet<String>) {
            let p = Projector::open(path, "test").unwrap();
            fold_log(&p);
            let g = p
                .subgraph(&["combat.rs".to_string(), "util.rs".to_string()], 3)
                .unwrap();
            let nodes = g.nodes.iter().map(node_desc).collect::<BTreeSet<_>>();
            let edges = g.edges.iter().map(edge_desc).collect::<BTreeSet<_>>();
            (nodes, edges)
        };

        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("graph.db");
        let (nodes, edges) = rebuild(first.to_str().unwrap());

        // The rebuilt projection is EXACTLY the code graph the log describes - every file container
        // node, every code-entity node with its re-derived attrs (definitions carry name/kind/line/
        // lang; a bare reference target carries none), and nothing spurious. Exact-set equality
        // pins both failure directions at once, so the rebuild neither under-derives nor
        // over-derives the nodes.
        let want_nodes: BTreeSet<String> = [
            "file combat.rs [lang=rust]",
            "file util.rs [lang=rust]",
            "code-entity combat.rs::apply_damage [kind=function,lang=rust,line=1,name=apply_damage]",
            "code-entity util.rs::shared [kind=function,lang=rust,line=1,name=shared]",
            "code-entity combat.rs::shared []",
            "code-entity combat.rs::magic []",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            nodes, want_nodes,
            "the rebuilt code graph re-derives EXACTLY the file and code-entity nodes the log \
             describes, with their attrs; got {nodes:?}"
        );

        // Every structural edge is re-derived at its confidence tier: a definition's containment
        // and a same-file reference EXTRACTED, the cross-file reference INFERRED (the convergent
        // upgrade, re-derived on replay), and a reference defined nowhere AMBIGUOUS.
        let want_edges: BTreeSet<String> = [
            "util.rs -CONTAINS-> util.rs::shared [extracted]",
            "combat.rs -CONTAINS-> combat.rs::apply_damage [extracted]",
            "combat.rs -REFERENCES-> combat.rs::apply_damage [extracted]",
            "combat.rs -REFERENCES-> combat.rs::shared [inferred]",
            "combat.rs -REFERENCES-> combat.rs::magic [ambiguous]",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            edges, want_edges,
            "the rebuilt code graph re-derives EXACTLY the tiered structural edges the log \
             describes; got {edges:?}"
        );

        // REBUILDABLE: discard that graph.db and fold the SAME log from scratch into a DIFFERENT
        // fresh, empty db. The re-derived nodes and tiered edges are IDENTICAL - the code graph is
        // a pure, reproducible function of the log, re-derived on every fold (including the
        // convergent tier upgrade), never a mutable side artifact a rebuild would drop.
        let dir2 = tempfile::tempdir().unwrap();
        let second = dir2.path().join("graph.db");
        let rebuilt_again = rebuild(second.to_str().unwrap());
        assert_eq!(
            (nodes, edges),
            rebuilt_again,
            "rebuilding the code log from scratch re-derives the identical nodes and tiered edges"
        );
    }
}
