//! The default context-graph projector: it folds the event log into bi-temporal
//! node and edge tables in a local SQLite file and answers Subgraph and Resolve.
//! A single connection behind a mutex serializes the read-then-write of apply.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::{
    CallEdge, CallGraph, CallNode, Direction, Edge, Error, Graph, Node, Projection,
    KIND_ARCH_DECISION, KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT,
    KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, KIND_FINDING, KIND_HANDBOOK_RULE, KIND_LESSON,
    KIND_RATIONALE, REL_ABOUT, REL_CALLS, REL_CONSTRAINS, REL_CONTAINS, REL_DOC_REFERENCES,
    REL_EXPLAINS, REL_GOVERNS, REL_IN_COMMUNITY, REL_RAISED, REL_REALIZES, REL_REFERENCES,
    REL_SPECIFIES, REL_SUPERSEDES, TIER_AMBIGUOUS, TIER_EXTRACTED, TIER_INFERRED,
    TYPE_ALIAS_DEFINED, TYPE_ALIAS_UNRESOLVED, TYPE_CODE_ENTITY_EXTRACTED, TYPE_COMMUNITY_ASSIGNED,
    TYPE_CONCEPT_DERIVED, TYPE_CONCEPT_REALIZED, TYPE_DECISION_MADE, TYPE_DOC_CONCEPT_EXTRACTED,
    TYPE_DOC_LINK_EXTRACTED, TYPE_EDGE_INFERRED, TYPE_FILE_TOUCHED, TYPE_GATE_VERDICT,
    TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING, TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED,
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

/// What one [`Projector::prune`] reclaimed, both in the same transaction: the dead-run
/// decision/finding nodes it dropped (spec 21) and the superseded structural edges it reclaimed
/// beyond the retention boundary (spec 41). A plain count pair so `rigger reset --runs` reports
/// both, and a test can assert the superseded-edge reclamation off the return value without
/// reaching into the private edge table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneStats {
    /// Dead-run / pre-boundary decision and finding nodes removed (spec 21).
    pub nodes: usize,
    /// Superseded structural edges (`valid_to IS NOT NULL`) reclaimed because they were retired
    /// before the retention boundary - a prior run's cruft the log can re-derive (spec 41). A LIVE
    /// edge (`valid_to IS NULL`) is never counted here, because it is never reclaimed.
    pub superseded_edges: usize,
}

/// The resolution of a `rigger graph --show <entity>` query (spec 58, the TEXT half of lookup).
/// [`Projector::locate`] resolves the query exactly the way the graph's other surfaces do - a full
/// `<file>::<name>` node id, or a bare name matched by the pinned name-suffix expression - and
/// returns one of three honest outcomes, never a guess among candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Located {
    /// Exactly one entity resolved: its definition site and graph facts, from which the CLI reads
    /// the line-numbered body out of the working tree.
    One(EntitySite),
    /// An ambiguous bare name (several definitions share it): the SORTED candidate sites the caller
    /// picks from. The show surface prints these and NO body (the call-views honesty rule).
    Many(Vec<Candidate>),
    /// Nothing in the graph matched the query.
    None,
}

/// A single located code entity (spec 58): where its definition lives and how it sits in the graph,
/// so the show surface can print the site header and bound the body it reads from the working tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitySite {
    /// The full `<file>::<name>` node id.
    pub id: String,
    /// The definition kind (`function`, `type`, ...), from the node's `kind` attr; falls back to
    /// the node's graph kind when a bare placeholder carries no definition attr.
    pub kind: String,
    /// The definition's file (the id prefix before `::`) - the working-tree path the body reads.
    pub file: String,
    /// The 1-based line of the definition site, from the node's `line` attr (`0` when unknown).
    pub line: u32,
    /// The entity's one-hop degree: the count of currently-live edges incident to it, so the reader
    /// knows how connected the entity is.
    pub degree: usize,
    /// The 1-based line at which the NEXT definition in the SAME file begins - the structural upper
    /// bound (exclusive) on this entity's body extent, since neither the graph node nor the symbol
    /// index records a true end line. `None` when this is the file's last definition.
    pub next_def_line: Option<u32>,
}

/// One disambiguation candidate for an ambiguous bare name (spec 58): the full node id and its
/// file. [`Projector::locate`] returns these SORTED by id, so the listing is deterministic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The candidate's full `<file>::<name>` node id.
    pub id: String,
    /// The candidate's file (the id prefix before `::`).
    pub file: String,
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
        migrate_indexes(&conn)?;
        Ok(Projector {
            conn: Mutex::new(conn),
            project: project.to_string(),
        })
    }

    /// The WHOLE live projection for this project: every node plus every currently-valid edge
    /// (`valid_to IS NULL`), read DIRECTLY (spec 45, criterion 2 - the direct-projection read the
    /// dedicated `/api/graph` provider consults). Unlike [`Projection::subgraph`] it takes no seed
    /// and walks no reachability CTE - it returns the full graph, so a whole-graph overview and a
    /// seeded neighborhood both reach any node the projection holds (the fix for the never-built
    /// repo whose run-seed set is empty). Read isolation (spec 28, criterion 2): scoped to
    /// `self.project` exactly like `subgraph`, so on a shared backend it returns only this project's
    /// nodes and edges. Deterministic by construction (spec 45): the rows are ORDERed so the read
    /// itself sorts, independent of any downstream fold. Read-only; a fresh/empty graph yields an
    /// empty [`Graph`], never an error.
    pub fn whole(&self) -> Result<Graph, Error> {
        let conn = self.conn.lock().unwrap();
        let mut nstmt = conn
            .prepare(
                "SELECT id, kind, attrs FROM nodes
                 WHERE project = ?1
                 ORDER BY id",
            )
            .map_err(be)?;
        let nodes: Vec<Node> = nstmt
            .query_map(params![self.project], row_to_node)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;

        let mut estmt = conn
            .prepare(
                "SELECT from_id, to_id, rel, valid_from, source, tier FROM edges
                 WHERE valid_to IS NULL AND project = ?1
                 ORDER BY from_id, to_id, rel, valid_from",
            )
            .map_err(be)?;
        let edges: Vec<Edge> = estmt
            .query_map(params![self.project], row_to_edge)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;

        Ok(Graph { nodes, edges })
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
    ///
    /// `superseded_before` extends the SAME prune authority to reclaim the superseded-edge
    /// accumulation (spec 41): when `Some(boundary)` it also reclaims every SUPERSEDED structural
    /// edge (`valid_to IS NOT NULL`) retired before `boundary` - a nanosecond-since-epoch cutoff in
    /// the graph's own [`to_nanos`] time base, derived by the composition root from the active run's
    /// `RunStarted` (the SAME run boundary that keeps the active run's nodes). Such an edge is a
    /// prior run's dead cruft: no live query reads it (grounding filters `valid_to IS NULL`) and the
    /// log re-derives it on a rebuild. A LIVE edge (`valid_to IS NULL`) is NEVER matched, so the safe
    /// superset grounding reads is untouched; an edge retired at or after the boundary (recent
    /// history) is kept, so the window is a bounded, not cumulative, tail. `None` reclaims no edge
    /// (a legacy store with no run boundary, or a node-only prune), preserving the pre-spec-41
    /// behavior. Project-scoped like the node deletes, and in the SAME transaction, so the whole
    /// prune is atomic.
    pub fn prune(
        &self,
        node_ids: &[String],
        superseded_before: Option<i64>,
    ) -> Result<PruneStats, Error> {
        let ids_json = serde_json::to_string(node_ids).map_err(be)?;
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction().map_err(be)?;
        // Delete every edge referencing a pruned node from EITHER end - a superseded
        // decision's DECIDED/GOVERNS/SUPERSEDES edges and a finding's ABOUT/RAISED edges,
        // whether currently valid or already invalidated - so no edge dangles to a gone node.
        // Scoped to THIS project (spec 28 criterion 3): on a shared backend another project may
        // hold an edge that shares a from_id/to_id with a pruned id, and `reset --runs` must
        // never touch it - the SAME injected project the fold stamps is the ONE scope. An empty
        // `node_ids` yields `[]`, so `json_each` matches nothing and this is a 0-row no-op - the
        // superseded-edge reclamation below still runs.
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
        let nodes = tx
            .execute(
                "DELETE FROM nodes
                 WHERE id IN (SELECT value FROM json_each(?1)) AND project = ?2",
                params![ids_json, self.project],
            )
            .map_err(be)?;
        // Spec 41: reclaim superseded structural edges retired before the retention boundary.
        // `valid_to IS NOT NULL` makes this LIVE-safe by construction - a live edge is never
        // matched, so grounding, blast-radius, and the two-view safe superset are untouched. STRICT
        // `<`: an edge whose `valid_to` equals the boundary was retired at the instant the active
        // run began, so it is recent history the window keeps, not cumulative cruft. Project-scoped
        // like the node deletes and in this same transaction, so the whole prune is atomic. `None`
        // reclaims nothing (a legacy store with no run boundary, or a node-only prune).
        let superseded_edges = match superseded_before {
            Some(before) => tx
                .execute(
                    "DELETE FROM edges
                     WHERE valid_to IS NOT NULL AND valid_to < ?1 AND project = ?2",
                    params![before, self.project],
                )
                .map_err(be)?,
            None => 0,
        };
        tx.commit().map_err(be)?;
        Ok(PruneStats {
            nodes,
            superseded_edges,
        })
    }

    /// Compact the on-disk graph file after a [`prune`], returning the bytes reclaimed (spec 46,
    /// criterion 3). [`prune`] drops superseded projection ROWS, but SQLite keeps the freed pages
    /// inside `graph.db` on a freelist for reuse, so the file stays as LARGE on disk as before the
    /// prune even though those rows are gone. `VACUUM` rebuilds the database without those free
    /// pages, then a `TRUNCATE` checkpoint folds the WAL side-file back so the MAIN file actually
    /// shrinks on disk immediately (not only at the next checkpoint), which is what a consumer
    /// inspecting the file then sees.
    ///
    /// This reclaims DISK ONLY. `VACUUM` changes NO query result and gives NO query or fold
    /// speedup: the row-count reduction that speeds reads is [`prune`]'s job, not `compact`'s. The
    /// projection is a persistent, incrementally-maintained file - `apply` folds only unseen log
    /// positions and `open` runs idempotent migrations without re-folding history - so a
    /// freelist-bloated file is never itself a slower query or a slower open; the only thing
    /// `compact` changes is the file's SIZE on disk.
    ///
    /// It is the DISK counterpart to `prune`'s ROW reclamation and the SAME graph-mutation
    /// authority - never a second connection opened elsewhere. It is a pure projection-file
    /// rebuild: it never touches the event log (the source of truth) and never changes a query
    /// result, only the file size, so it is safe to run unconditionally after every prune. The
    /// reclaimed byte count is the drop in `page_count * page_size` across the `VACUUM`; a
    /// no-op-shaped compaction (nothing was freed) simply reclaims 0 bytes.
    pub fn compact(&self) -> Result<u64, Error> {
        let conn = self.conn.lock().unwrap();
        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .map_err(be)?;
        // `page_count` counts the freelist pages the prune's deletes freed, so this is the
        // pre-VACUUM file size; after the VACUUM it is the compacted count.
        let before: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .map_err(be)?;
        conn.execute_batch("VACUUM").map_err(be)?;
        // Fold the WAL back into the main file so the shrink lands on disk now, not at some later
        // checkpoint - the reported reclamation must match the file size on disk.
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(be)?;
        let after: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .map_err(be)?;
        let reclaimed_pages = before.saturating_sub(after).max(0) as u64;
        Ok(reclaimed_pages * page_size.max(0) as u64)
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

    /// Resolve a `rigger graph --show <entity>` query to a located entity, a candidate list, or
    /// nothing (spec 58, criterion 1). It serves from the SAME node/edge tables every graph surface
    /// reads - reusing [`node_row`] and the pinned [`definitions_with_suffix`] name-suffix match -
    /// so the show surface never forks a second resolution authority.
    ///
    /// Resolution order (the graph's existing surface convention): an EXACT node-id match first, so
    /// a full `<file>::<name>` id resolves directly; only on a miss is the input treated as a BARE
    /// name and matched by the pinned name-suffix expression over code-entity DEFINITION nodes.
    /// Zero candidates -> [`Located::None`]; exactly one -> [`Located::One`] (its site); more than
    /// one -> [`Located::Many`] the SORTED candidates, never a guess among them.
    ///
    /// Read-only over the projection; a missing / drifted working-tree location is NOT this method's
    /// concern (it reports the recorded graph site) - the CLI reads and, if stale, degrades the body.
    pub fn locate(&self, entity: &str) -> Result<Located, Error> {
        let conn = self.conn.lock().unwrap();
        // Exact id first: a full `<file>::<name>` id resolves directly to its node.
        if let Some(node) = node_row(&conn, entity, &self.project)? {
            return Ok(Located::One(self.site_of(&conn, node)?));
        }
        // Else a bare name: the pinned name-suffix match over definition nodes (sorted by id).
        let cands = definitions_with_suffix(&conn, entity, &self.project)?;
        match cands.as_slice() {
            [] => Ok(Located::None),
            [only] => {
                let node = node_row(&conn, only, &self.project)?.ok_or_else(|| {
                    Error(format!("locate: candidate {only:?} vanished between reads"))
                })?;
                Ok(Located::One(self.site_of(&conn, node)?))
            }
            many => Ok(Located::Many(
                many.iter()
                    .map(|id| Candidate {
                        file: file_prefix(id).to_string(),
                        id: id.clone(),
                    })
                    .collect(),
            )),
        }
    }

    /// Build an [`EntitySite`] from a resolved node: its file (the id prefix), its recorded line and
    /// kind (attrs), its one-hop live-edge degree, and the next-definition line in the same file
    /// (the body's structural upper bound). Read-only; used by [`locate`](Projector::locate).
    fn site_of(&self, conn: &Connection, node: Node) -> Result<EntitySite, Error> {
        let file = file_prefix(&node.id).to_string();
        let line = node
            .attrs
            .get("line")
            .and_then(|l| l.parse::<u32>().ok())
            .unwrap_or(0);
        let kind = node
            .attrs
            .get("kind")
            .cloned()
            .unwrap_or_else(|| node.kind.clone());
        let degree = one_hop_degree(conn, &node.id, &self.project)?;
        let next_def_line = next_definition_line(conn, &file, line, &self.project)?;
        Ok(EntitySite {
            id: node.id,
            kind,
            file,
            line,
            degree,
            next_def_line,
        })
    }

    /// The DOWN direction of [`Projection::calls`] (spec 52 criterion 1): the execution path out of
    /// the seed. A breadth-first walk by LAYER over the live, caller-attributed `CALLS` edges
    /// (spec 37), following callees transitively. It answers "what does this call" as a directed,
    /// layered DAG - the thing the undirected [`subgraph`](Projection::subgraph) cannot, because a
    /// naive forward walk stops at the first file boundary where a cross-file call points at a BARE
    /// placeholder node (the definition lives in another file's namespace).
    ///
    /// - **Layers.** The seed is layer 0; each callee reached for the FIRST time takes its
    ///   discoverer's layer + 1. BFS visits shallowest-first, so a node's recorded layer is its
    ///   minimum hop distance from the seed, stable across polls.
    /// - **Cross-file resolution.** A `CALLS` edge whose callee target is BARE (no `name` attr - the
    ///   definition is in another file) is resolved by the pinned name-suffix expression over
    ///   code-entity DEFINITION nodes: EXACTLY ONE definition auto-continues (the edge is redirected
    ///   onto the real definition); MORE THAN ONE becomes a marked frontier ([`CallNode::frontier`]
    ///   carrying the SORTED candidate ids) the walk does NOT descend - honest by construction, the
    ///   human re-seeds on a chosen candidate; ZERO leaves the bare node a terminal leaf. A same-file
    ///   callee already lands on its real definition, so it needs no resolution.
    /// - **Cycles / dedup.** Reached nodes dedup (a node is expanded at most once), so recursion and
    ///   mutual calls TERMINATE into a DAG instead of looping. An edge whose target layer is NOT
    ///   deeper than its source is marked a BACK edge ([`CallEdge::back`]) rather than duplicated.
    /// - **Tier floor.** An edge is followed only when its confidence tier is at or above
    ///   `tier_floor`; the default (an empty / unrecognized value or [`TIER_INFERRED`]) is the
    ///   resolvable floor that excludes [`TIER_AMBIGUOUS`], and passing [`TIER_AMBIGUOUS`] opts the
    ///   unresolved tier in.
    /// - **Determinism / isolation.** Neighbor and candidate reads are `ORDER BY` sorted and the
    ///   result nodes/edges are sorted (by layer then id, and by endpoints), so the same graph and
    ///   seed yield a byte-identical [`CallGraph`] across polls. Every read is scoped to
    ///   `self.project` exactly like [`subgraph`](Projection::subgraph). A missing seed, or a seed
    ///   with no calls, yields an empty view - never an error.
    fn calls_down(
        &self,
        seed: &[String],
        depth: i64,
        tier_floor: &str,
    ) -> Result<CallGraph, Error> {
        let conn = self.conn.lock().unwrap();
        let floor = tier_floor_rank(tier_floor);

        // `layer_of` records each REACHED node's min hop distance from the seed (its final layer);
        // it also serves as the visited set, so a node is expanded at most once - recursion and
        // mutual calls dedup into a DAG. `frontier_of` holds, for a multi-candidate hop the walk
        // did NOT descend, its sorted candidate ids.
        let mut layer_of: BTreeMap<String, i64> = BTreeMap::new();
        let mut frontier_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut call_edges: Vec<CallEdge> = Vec::new();
        let mut queue: VecDeque<(String, i64)> = VecDeque::new();

        // The seed sits at layer 0. Only ids that EXIST as nodes in this project are seeded (a
        // missing seed yields an empty view); deduped and ordered so the walk is deterministic.
        let mut seeds: Vec<String> = seed.to_vec();
        seeds.sort();
        seeds.dedup();
        for s in seeds {
            if layer_of.contains_key(&s) {
                continue;
            }
            if node_row(&conn, &s, &self.project)?.is_some() {
                layer_of.insert(s.clone(), 0);
                queue.push_back((s, 0));
            }
        }

        // Breadth-first by layer: dequeuing shallowest-first makes each node's recorded layer its
        // MINIMUM hop distance, and bounds the walk to `depth` hops.
        while let Some((cur, cur_layer)) = queue.pop_front() {
            if cur_layer >= depth {
                continue; // depth clamp: do not expand a node at the bound
            }
            for (raw_to, tier, valid_from, source) in calls_out(&conn, &cur, &self.project)? {
                if tier_rank(&tier) < floor {
                    continue; // below the confidence floor: not a followed edge
                }
                // Resolve the callee: a same-file definition lands directly; a bare cross-file
                // placeholder resolves by its name-suffix to the definition(s) sharing the name.
                let (target, frontier) = resolve_down_hop(&conn, &raw_to, &self.project)?;
                let newly = !layer_of.contains_key(&target);
                let target_layer = if newly {
                    cur_layer + 1
                } else {
                    layer_of[&target]
                };
                if newly {
                    layer_of.insert(target.clone(), target_layer);
                    match &frontier {
                        // A frontier is marked but NEVER descended - the human re-seeds on a chosen
                        // candidate, so the walk never guesses which definition a name resolves to.
                        Some(cands) => {
                            frontier_of.insert(target.clone(), cands.clone());
                        }
                        None => queue.push_back((target.clone(), target_layer)),
                    }
                }
                // A recursion / mutual-call edge points at a node no deeper than its source: mark it
                // a BACK edge rather than following the target a second time (the DAG already holds
                // it).
                let back = target_layer <= cur_layer;
                call_edges.push(CallEdge {
                    edge: Edge {
                        from: cur.clone(),
                        to: target.clone(),
                        rel: REL_CALLS.to_string(),
                        valid_from,
                        valid_to: None,
                        source,
                        tier,
                    },
                    back,
                });
            }
        }

        // Materialize the reached nodes (fetch kind/attrs) with their layer and any frontier
        // marker, ordered by (layer, id); sort the edges by endpoints. Deterministic by
        // construction, so the same graph and seed yield a byte-identical result across polls.
        let mut nodes: Vec<CallNode> = Vec::with_capacity(layer_of.len());
        for (id, layer) in &layer_of {
            if let Some(node) = node_row(&conn, id, &self.project)? {
                nodes.push(CallNode {
                    node,
                    layer: *layer,
                    frontier: frontier_of.get(id).cloned(),
                });
            }
        }
        nodes.sort_by(|a, b| {
            a.layer
                .cmp(&b.layer)
                .then_with(|| a.node.id.cmp(&b.node.id))
        });
        call_edges.sort_by(|a, b| {
            a.edge
                .from
                .cmp(&b.edge.from)
                .then_with(|| a.edge.to.cmp(&b.edge.to))
                .then_with(|| a.edge.rel.cmp(&b.edge.rel))
        });

        Ok(CallGraph {
            nodes,
            edges: call_edges,
            // The "referenced but not called" sidecar is an UP-direction concept (who imports/uses
            // the seed without calling it); the DOWN execution path never carries it.
            referenced_not_called: Vec::new(),
        })
    }

    /// The UP direction of [`Projection::calls`] (spec 52 criterion 3): the CALL SITES into the seed.
    /// A breadth-first walk by LAYER over the live, caller-attributed `CALLS` edges (spec 37), this
    /// time following CALLERS transitively - the REVERSE of [`calls_down`](Projector::calls_down). It
    /// answers "who calls this, transitively" as a directed, layered DAG, resolving THROUGH the bare
    /// cross-file placeholder nodes a naive reverse walk would never connect (a cross-file caller's
    /// call target lives in the CALLER's file namespace as a bare placeholder, not on the seed's
    /// definition).
    ///
    /// - **Layers.** The seed is layer 0; each caller reached for the FIRST time takes its callee's
    ///   layer + 1. BFS visits shallowest-first, so a caller's recorded layer is its minimum hop
    ///   distance from the seed, stable across polls. The left-to-right renderer draws the seed on the
    ///   RIGHT with callers flowing in from deeper layers.
    /// - **Reverse cross-file resolution (the mirror of the DOWN policy).** A caller of the seed's
    ///   name is found through the reverse name-match: an edge that literally targets `cur` is a
    ///   SAME-FILE caller (its call already lands on the definition - always followed, unambiguous);
    ///   an edge to a BARE placeholder whose entity-name equals `cur`'s name is a CROSS-FILE caller,
    ///   attributed to `cur` only when that name has EXACTLY ONE definition (`cur` itself). When the
    ///   name has MORE THAN ONE definition the caller is a marked FRONTIER ([`CallNode::frontier`]
    ///   carrying the SORTED candidate definition ids) the walk does NOT ascend - honest by
    ///   construction, since that caller might be calling a same-named sibling rather than the seed;
    ///   the human re-seeds on a chosen candidate.
    /// - **Cycles / dedup.** Reached nodes dedup (a node is expanded at most once), so recursion and
    ///   mutual calls TERMINATE into a DAG. An edge whose discovered caller sits at a layer NOT deeper
    ///   than the callee it was found from is marked a BACK edge ([`CallEdge::back`]) rather than
    ///   ascended a second time.
    /// - **Referenced-but-not-called.** Beyond the traversed caller DAG, the UP view carries a flat,
    ///   NON-traversed [`CallGraph::referenced_not_called`] list: the FILE nodes that reference the
    ///   seed's name at file level but call it from no function within them (top-level imports / uses)
    ///   - the who-uses-this sites the caller DAG deliberately excludes.
    /// - **Tier floor / determinism / isolation.** Identical to [`calls_down`](Projector::calls_down):
    ///   an edge is followed only at/above `tier_floor`; neighbor and candidate reads are `ORDER BY`
    ///   sorted and the result is sorted, so the same graph and seed yield a byte-identical
    ///   [`CallGraph`] across polls; every read is scoped to `self.project`; a missing seed, or a seed
    ///   nothing calls, yields an empty / seed-only view - never an error.
    fn calls_up(&self, seed: &[String], depth: i64, tier_floor: &str) -> Result<CallGraph, Error> {
        let conn = self.conn.lock().unwrap();
        let floor = tier_floor_rank(tier_floor);

        // `layer_of` records each REACHED node's min hop distance from the seed and doubles as the
        // visited set (a node is expanded at most once - recursion and mutual calls dedup into a
        // DAG); `frontier_of` holds, for a caller whose call to its callee was multi-candidate, the
        // sorted candidate definition ids the walk did NOT ascend.
        let mut layer_of: BTreeMap<String, i64> = BTreeMap::new();
        let mut frontier_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut call_edges: Vec<CallEdge> = Vec::new();
        let mut queue: VecDeque<(String, i64)> = VecDeque::new();

        // The seed sits at layer 0. Only ids that EXIST as nodes in this project are seeded (a
        // missing seed yields an empty view); deduped and ordered so the walk is deterministic.
        let mut seeds: Vec<String> = seed.to_vec();
        seeds.sort();
        seeds.dedup();
        for s in seeds {
            if layer_of.contains_key(&s) {
                continue;
            }
            if node_row(&conn, &s, &self.project)?.is_some() {
                layer_of.insert(s.clone(), 0);
                queue.push_back((s, 0));
            }
        }

        // Breadth-first by layer over CALLERS: dequeuing shallowest-first makes each caller's
        // recorded layer its MINIMUM hop distance, and bounds the walk to `depth` hops.
        while let Some((cur, cur_layer)) = queue.pop_front() {
            if cur_layer >= depth {
                continue; // depth clamp: do not expand a node at the bound
            }
            for (caller, tier, valid_from, source, frontier) in
                callers_of(&conn, &cur, &self.project)?
            {
                if tier_rank(&tier) < floor {
                    continue; // below the confidence floor: not a followed edge
                }
                let newly = !layer_of.contains_key(&caller);
                let caller_layer = if newly {
                    cur_layer + 1
                } else {
                    layer_of[&caller]
                };
                if newly {
                    layer_of.insert(caller.clone(), caller_layer);
                    match &frontier {
                        // A frontier caller is marked but NEVER ascended - it might call a same-named
                        // sibling rather than the seed, so the walk never guesses; the human re-seeds
                        // on a chosen candidate definition.
                        Some(cands) => {
                            frontier_of.insert(caller.clone(), cands.clone());
                        }
                        None => queue.push_back((caller.clone(), caller_layer)),
                    }
                }
                // A recursion / mutual-call edge whose caller is no deeper than the callee it was
                // found from: mark it BACK rather than ascending the caller a second time (the DAG
                // already holds it). The edge keeps its real CALLS direction (caller -> callee).
                let back = caller_layer <= cur_layer;
                call_edges.push(CallEdge {
                    edge: Edge {
                        from: caller.clone(),
                        to: cur.clone(),
                        rel: REL_CALLS.to_string(),
                        valid_from,
                        valid_to: None,
                        source,
                        tier,
                    },
                    back,
                });
            }
        }

        // Materialize the reached nodes (fetch kind/attrs) with their layer and any frontier marker,
        // ordered by (layer, id); sort the edges by endpoints. Deterministic by construction.
        let mut nodes: Vec<CallNode> = Vec::with_capacity(layer_of.len());
        for (id, layer) in &layer_of {
            if let Some(node) = node_row(&conn, id, &self.project)? {
                nodes.push(CallNode {
                    node,
                    layer: *layer,
                    frontier: frontier_of.get(id).cloned(),
                });
            }
        }
        nodes.sort_by(|a, b| {
            a.layer
                .cmp(&b.layer)
                .then_with(|| a.node.id.cmp(&b.node.id))
        });
        call_edges.sort_by(|a, b| {
            a.edge
                .from
                .cmp(&b.edge.from)
                .then_with(|| a.edge.to.cmp(&b.edge.to))
                .then_with(|| a.edge.rel.cmp(&b.edge.rel))
        });

        let referenced_not_called = referenced_not_called(&conn, seed, &self.project)?;

        Ok(CallGraph {
            nodes,
            edges: call_edges,
            referenced_not_called,
        })
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

/// Additive read-path indexes (spec 45, unit 4). Two pure additions - no column, no data
/// migration, no result change - that keep the whole-graph and directed-call reads sub-linear as a
/// repository grows. Run AFTER [`migrate_project_scope`] and [`migrate_edge_tier`] so both indexes
/// land on the FINAL table shapes: on a pre-spec-28 graph.db the scope migration DROPS and recreates
/// `nodes`, which would strand a `nodes` index created any earlier, so this must fire last. Unlike
/// the column migrations, `CREATE INDEX IF NOT EXISTS` is inherently idempotent, so it needs no
/// [`column_exists`] guard - a fresh db and an existing one both gain both indexes on open, and a
/// re-open is a no-op.
///
/// - `idx_edges_live_rel_from` is a PARTIAL index on `(rel, from_id) WHERE valid_to IS NULL`: the
///   relationship-scoped forward scan a directed CALLS/REFERENCES traversal (spec 46) walks reads
///   only LIVE edges, so restricting the index to `valid_to IS NULL` keeps it small and lets SQLite
///   seek by `rel` then `from_id` instead of scanning every historical edge. A query MUST carry the
///   exact `valid_to IS NULL` term for SQLite to use a partial index.
/// - `idx_nodes_name_suffix` is an EXPRESSION index on `substr(id, instr(id, '::') + 2)` - the
///   entity-name suffix of a `<file>::<name>` [`code_entity_id`]. This is the SAME expression the
///   convergent-tier-upgrade fold already uses on the edge side (`substr(to_id, instr(to_id, '::') +
///   2)`), pinned here on `nodes.id` so the coming cross-file name resolution resolves a bare name to
///   its definition node via an index seek, not a full scan. SQLite uses an expression index only
///   when the query's expression MATCHES the indexed one, so the resolution query must be phrased
///   with this identical expression or it silently misses the index.
fn migrate_indexes(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_edges_live_rel_from
             ON edges(rel, from_id) WHERE valid_to IS NULL;
         CREATE INDEX IF NOT EXISTS idx_nodes_name_suffix
             ON nodes(substr(id, instr(id, '::') + 2));",
    )
    .map_err(be)
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

    /// Fold a whole batch of events in ONE transaction (spec 49's batched-fold cadence): the store's
    /// transaction cost is paid ONCE for the whole file batch instead of once per event, which is the
    /// load-bearing fix for a cold `graph build` whose measured throughput was transaction-cadence
    /// bound. The result is IDENTICAL to folding each event with [`apply`](Projection::apply) in
    /// order - same per-position idempotency guard (`applied`), same fold - so batching alters
    /// CADENCE only, never the graph. Atomic: a fold error rolls the whole batch back (the events are
    /// still durable in the log, and the sink folds best-effort), never a half-applied batch.
    fn apply_batch(&self, events: &[Event]) -> Result<(), Error> {
        if events.is_empty() {
            return Ok(());
        }
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction().map_err(be)?;
        for e in events {
            let inserted = tx
                .execute(
                    "INSERT OR IGNORE INTO applied (position) VALUES (?1)",
                    [e.position as i64],
                )
                .map_err(be)?;
            if inserted > 0 {
                fold(&tx, e, &self.project)?;
            }
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

    /// Directed `CALLS` traversal (spec 52): dispatch on direction. `Down` (callees - the execution
    /// path) runs the forward layered walk; `Up` (callers - the call sites) runs the reverse layered
    /// walk plus the referenced-but-not-called sidecar (spec 52 criterion 3). Read isolation and
    /// clamping match [`subgraph`]: both walks stay scoped to `self.project` and bounded by `depth`.
    fn calls(
        &self,
        seed: &[String],
        direction: Direction,
        depth: i64,
        tier_floor: &str,
    ) -> Result<CallGraph, Error> {
        match direction {
            Direction::Down => self.calls_down(seed, depth, tier_floor),
            Direction::Up => self.calls_up(seed, depth, tier_floor),
        }
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
            // De-noise (spec 43): the graph models the TARGET PROJECT, not the loop's own
            // machinery, so no fold produces a KIND_AGENT node or an agent-attribution edge.
            // The acting persona (event actor) is NOT projected - the decision's CONTENT and its
            // GOVERNS edges to the code it concerns are what the graph is about. The actor stays
            // in the log for metrics and the run-tree, which read events, not this projection.
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
            // De-noise (spec 43): `agent --TOUCHES--> file` is run machinery, not the target
            // project's structure, so this arm is a graph no-op. The FileTouched event stays in
            // the log (metrics and the run-tree read it); it just no longer projects a KIND_AGENT
            // node or a REL_TOUCHES edge. The file's node, when it is one, is created by the code
            // structure folds and by the decisions/findings that GOVERN / are ABOUT it.
        }
        TYPE_GATE_VERDICT => {
            // De-noise (spec 43): a gate is run machinery, not the target project, so this arm is
            // a graph no-op - no KIND_GATE node, no REL_GATED_BY edge. The GateVerdict event stays
            // in the log, where metrics (per-gate remediation counts) and the run-tree read it.
        }
        TYPE_UNIT_STARTED => {
            // De-noise (spec 43): a unit is run machinery, not the target project, so this arm is
            // a graph no-op - no KIND_UNIT node, no KIND_AGENT node, no REL_ASSIGNED_TO / REL_BLOCKS
            // edge. The UnitStarted event stays in the log; the run-tree projects units/stages
            // straight from events (its proper home), and metrics reads it for units-started.
        }
        TYPE_UNIT_INTEGRATED => {
            let u: super::UnitIntegrated = serde_json::from_slice(&e.data).map_err(be)?;
            // De-noise (spec 43): no KIND_UNIT node is projected (a unit is run machinery). But the
            // integrate still drives disposition-expiry below - the LIFECYCLE the fold owns, which
            // reads the finding's `$.unit` string attribute (a token, never a KIND_UNIT node) and
            // is therefore unaffected by dropping the unit node.
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
            // them). De-noise (spec 43): the reviewer's provenance is NOT projected as a
            // KIND_AGENT node or a REL_RAISED edge - that agent attribution is run
            // machinery, not the target project. The `by` reviewer stays as a node
            // ATTRIBUTE on the finding (read by consumers off the content node), and the
            // event's actor stays in the log for metrics and the run-tree.
            let f: super::ReviewFinding = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &f.id,
                KIND_FINDING,
                &[("summary", &f.summary), ("by", &f.by), ("unit", &f.unit)],
                project,
            )?;
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
        TYPE_COMMUNITY_ASSIGNED => {
            // Spec 53 criterion 3 (the EVENT-SOURCED FOLD): one membership the offline
            // community-detection pass emitted. Fold it into a KIND_COMMUNITY super-node plus a live
            // `<node> --IN_COMMUNITY--> <community>` edge, so the derived coupling grouping lives in
            // the event-sourced projection (never computed at request time) and the `lens=code` view
            // is a pure read over it. ALWAYS compiled: the light lane folds a community log with the
            // detection pass absent, which is why the node kind, the relation, and this arm live
            // outside the feature that gates detection - mirroring the 29a CodeEntityExtracted arm.
            // Project-scoped like every arm (spec 28).
            let c: super::CommunityAssigned = serde_json::from_slice(&e.data).map_err(be)?;
            // The resolution grain is the f64's canonical string (the emit contract: the community
            // id's grain segment equals this), so the `fresh` reset and the stored attr agree.
            let res = format!("{}", c.resolution);
            // Re-run supersession (rides `fresh` on the pass's FIRST event, mirroring
            // supersede_file_edges): retire (set `valid_to` on, never delete) every LIVE
            // IN_COMMUNITY edge of THIS resolution grain BEFORE folding the new pass, so a re-run at
            // a resolution REPLACES that grain's assignment set rather than accreting - and leaves
            // every OTHER grain's memberships live (scoped by the exact `community/<res>/` id prefix,
            // a substr equality, never a LIKE/GLOB whose wildcards a value could carry). A no-op on
            // the first-ever pass (no prior memberships). This is the fold-side supersession the
            // grain criterion relies on; the criterion 3 recording discipline owns the mechanism.
            // An EMPTY re-run never reaches here (an empty assignment records no events, so no `fresh`
            // event fires): that is the deliberate KEEP-LAST-GOOD policy - the prior non-empty
            // assignment stays live - documented on `community::events` (d-u53c2-empty-rerun-keep-last-good).
            if c.fresh {
                let prefix = format!("community/{res}/");
                tx.execute(
                    "UPDATE edges SET valid_to = ?1
                     WHERE valid_to IS NULL AND project = ?4 AND rel = ?3
                       AND substr(to_id, 1, length(?2)) = ?2",
                    params![at, prefix, REL_IN_COMMUNITY, project],
                )
                .map_err(be)?;
                // Node-side supersession (the completeness half): the edge retire above leaves every
                // community super-node of THIS grain with no live member, so a re-run that DROPS or
                // EMPTIES a community (a shrink, or the last member moving out of one) would strand
                // its KIND_COMMUNITY node - a ghost/orphan bucket that `whole()` (the read the dash
                // `/api/graph` provider consults) still surfaces with ZERO live members and a STALE
                // label. Drop every now-memberless community node OF THIS GRAIN so no such ghost
                // survives; the pass's own events immediately re-`ensure` exactly the communities its
                // NEW assignment uses (the first member of each folds its node back), leaving only the
                // dropped/emptied communities gone. Nodes carry no `valid_to`, so this is a DELETE
                // (the same node-removal primitive `prune` uses), not an edge-style retire. Scoped by
                // the SAME `community/<res>/` id prefix (a substr equality, never a wildcard) so it
                // never touches another grain's nodes, and by KIND_COMMUNITY so only derived
                // super-nodes are eligible; the `NOT EXISTS(live member)` guard makes the intent
                // exact - a community is dropped only once its last live member is gone. A rebuild
                // replays the same events in the same order and re-derives the identical node set.
                tx.execute(
                    "DELETE FROM nodes
                      WHERE kind = ?2 AND project = ?3
                        AND substr(id, 1, length(?1)) = ?1
                        AND NOT EXISTS (
                            SELECT 1 FROM edges e
                             WHERE e.to_id = nodes.id AND e.rel = ?4
                               AND e.valid_to IS NULL AND e.project = ?3
                        )",
                    params![prefix, KIND_COMMUNITY, project, REL_IN_COMMUNITY],
                )
                .map_err(be)?;
            }
            // The community super-node (first-writer-wins kind; its attrs are set below once the
            // deterministic label is computed). A bare ensure keeps any attrs an earlier member of
            // the same community already wrote until this fold overwrites them with the recomputed
            // label over the now-larger live membership.
            ensure_node(tx, &c.community, KIND_COMMUNITY, &[], project)?;
            // The membership edge, a DERIVED grouping (TIER_INFERRED - one confidence step below the
            // explicit structural edges detection runs over). Upsert-live like every fold (spec 40).
            add_edge(
                tx,
                &c.node,
                &c.community,
                REL_IN_COMMUNITY,
                at,
                e.position,
                project,
                TIER_INFERRED,
            )?;
            // Deterministic label: the community node's label is its highest-degree LIVE member's
            // label (its `name` attr, else its id), ties broken to the lexicographically-smallest
            // label - the dominant-kind tie-break discipline the overview uses. Degree is the count
            // of live structural edges (CALLS / REFERENCES / CONTAINS - the coupling layer detection
            // runs over) incident to the member, project-scoped. Recomputed over the community's
            // CURRENT live members on every fold, so after the pass's last member folds the label is
            // correct over the whole membership; and because it reads only the folded graph (which,
            // for a rebuild, is byte-identical at every step), a rebuild re-derives the SAME label -
            // nothing waits on a model. `max`/`min` are order-independent, keeping the derivation a
            // pure function of the log.
            let label: Option<String> = tx
                .query_row(
                    "SELECT COALESCE(json_extract(n.attrs, '$.name'), n.id) AS lbl
                       FROM edges m
                       JOIN nodes n ON n.id = m.from_id AND n.project = ?2
                      WHERE m.to_id = ?1 AND m.rel = ?3 AND m.valid_to IS NULL AND m.project = ?2
                      ORDER BY (
                          SELECT COUNT(*) FROM edges d
                           WHERE d.project = ?2 AND d.valid_to IS NULL
                             AND d.rel IN (?4, ?5, ?6)
                             AND (d.from_id = m.from_id OR d.to_id = m.from_id)
                      ) DESC, lbl ASC
                      LIMIT 1",
                    params![
                        c.community,
                        project,
                        REL_IN_COMMUNITY,
                        REL_CALLS,
                        REL_REFERENCES,
                        REL_CONTAINS
                    ],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(be)?;
            // Write the community node's attrs in one deterministic blob (serde_json sorts keys, so
            // the stored json is byte-stable across folds and rebuilds). All values are strings, the
            // shape Node.attrs (a String map) and the direct-read providers expect.
            let attrs = serde_json::json!({
                "resolution": res,
                "hash": c.hash,
                "label": label.unwrap_or_default(),
            })
            .to_string();
            tx.execute(
                "UPDATE nodes SET attrs = ?1 WHERE id = ?2 AND project = ?3",
                params![attrs, c.community, project],
            )
            .map_err(be)?;
        }
        TYPE_CONCEPT_DERIVED => {
            // Spec 54 (the CONCEPTS lens): one concept the offline intent-derivation pass emitted.
            // Fold it into a KIND_CONCEPT super-node carrying the pass-computed label, so the derived
            // grouping lives in the event-sourced projection (never computed at request time) and the
            // `lens=concepts` view is a pure read over it. ALWAYS compiled, mirroring the 53
            // CommunityAssigned arm: the light lane folds a concept log with the derivation pass
            // absent. Project-scoped like every arm (spec 28).
            let c: super::ConceptDerived = serde_json::from_slice(&e.data).map_err(be)?;
            // The resolution grain is the f64's canonical string (the emit contract: the concept id's
            // grain segment equals this), so the `fresh` reset and the stored attr agree.
            let res = format!("{}", c.resolution);
            // Re-run supersession (rides `fresh` on the pass's FIRST event, mirroring the community
            // arm): retire (set `valid_to` on, never delete) every LIVE REALIZES edge of THIS
            // resolution grain, then drop the grain's now-orphan concept nodes, BEFORE folding the new
            // pass - so a re-run at a resolution REPLACES that grain's grouping rather than accreting,
            // and leaves every OTHER grain's memberships live (scoped by the exact `concept/<res>/` id
            // prefix, a substr equality, never a LIKE/GLOB whose wildcards a value could carry). A
            // no-op on the first-ever pass. The pass emits all its ConceptDerived events before any
            // ConceptRealized, so this boundary fires once, at the head, before this pass's grouping
            // folds. An EMPTY re-run never reaches here (an empty derivation records no events, so no
            // `fresh` event fires): the KEEP-LAST-GOOD policy documented on `concepts::events`.
            if c.fresh {
                let prefix = format!("concept/{res}/");
                tx.execute(
                    "UPDATE edges SET valid_to = ?1
                     WHERE valid_to IS NULL AND project = ?4 AND rel = ?3
                       AND substr(to_id, 1, length(?2)) = ?2",
                    params![at, prefix, REL_REALIZES, project],
                )
                .map_err(be)?;
                // Node-side supersession (the completeness half, mirroring the community arm): the
                // edge retire above leaves every concept super-node of THIS grain with no live member,
                // so a re-run that DROPS or EMPTIES a concept would strand its KIND_CONCEPT node - a
                // ghost bucket `whole()` still surfaces with ZERO live members and a STALE label. Drop
                // every now-memberless concept node OF THIS GRAIN; this pass's own ConceptDerived
                // events immediately re-`ensure` exactly the concepts its NEW grouping uses, leaving
                // only the dropped/emptied concepts gone. Scoped by the SAME `concept/<res>/` id prefix
                // and by KIND_CONCEPT, with a `NOT EXISTS(live member)` guard, so it never touches
                // another grain's nodes. A rebuild replays the same events in the same order and
                // re-derives the identical node set.
                tx.execute(
                    "DELETE FROM nodes
                      WHERE kind = ?2 AND project = ?3
                        AND substr(id, 1, length(?1)) = ?1
                        AND NOT EXISTS (
                            SELECT 1 FROM edges e
                             WHERE e.to_id = nodes.id AND e.rel = ?4
                               AND e.valid_to IS NULL AND e.project = ?3
                        )",
                    params![prefix, KIND_CONCEPT, project, REL_REALIZES],
                )
                .map_err(be)?;
            }
            // The concept super-node, carrying its pass-computed label + provenance attrs. Unlike the
            // community arm (which recomputes a label from the folded members), the label is KNOWN
            // upfront - it rides on the event - so a single `ensure_node` with the attrs suffices; the
            // attrs blob is a sorted-key BTreeMap render (byte-stable across folds and rebuilds).
            ensure_node(
                tx,
                &c.concept,
                KIND_CONCEPT,
                &[
                    ("resolution", res.as_str()),
                    ("hash", c.hash.as_str()),
                    ("label", c.label.as_str()),
                ],
                project,
            )?;
        }
        TYPE_CONCEPT_REALIZED => {
            // Spec 54: one concept membership the offline intent-derivation pass emitted. Fold it into
            // a live `<node> --REALIZES--> <concept>` edge, a DERIVED grouping (TIER_INFERRED - one
            // confidence step below the explicit intent edges the derivation runs over). The concept
            // super-node already exists (its ConceptDerived folded earlier in the pass); ensure it
            // defensively (a bare ensure is idempotent and first-writer-wins keeps the Derived label
            // attrs) so the edge never dangles if the events are replayed out of the pass's order.
            // Upsert-live like every fold (spec 40). ALWAYS compiled, mirroring the community arm.
            let r: super::ConceptRealized = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &r.concept, KIND_CONCEPT, &[], project)?;
            add_edge(
                tx,
                &r.node,
                &r.concept,
                REL_REALIZES,
                at,
                e.position,
                project,
                TIER_INFERRED,
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

/// The entity-name suffix of a `<file>::<name>` id (spec 52): everything after the FIRST `::`. A
/// file path never contains `::`, so this is exactly the callee/definition name - the twin of the
/// SQL `substr(id, instr(id, '::') + 2)` the name-suffix expression index is built on. An id with
/// no `::` (never a code-entity id) is returned whole.
fn name_suffix(id: &str) -> &str {
    match id.find("::") {
        Some(i) => &id[i + 2..],
        None => id,
    }
}

/// The file prefix of a `<file>::<name>` id (spec 58): everything BEFORE the first `::` - the twin
/// of [`name_suffix`]. A file path never contains `::`, so this is exactly the definition's file.
/// An id with no `::` (never a code-entity id) is returned whole.
fn file_prefix(id: &str) -> &str {
    match id.find("::") {
        Some(i) => &id[..i],
        None => id,
    }
}

/// The one-hop degree of a node (spec 58): the count of currently-LIVE edges (`valid_to IS NULL`)
/// incident to `id` in `project`, in either direction. Read-only; the show surface prints it so a
/// reader knows how connected the entity is. A fresh / isolated node yields `0`.
fn one_hop_degree(conn: &Connection, id: &str, project: &str) -> Result<usize, Error> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges
              WHERE valid_to IS NULL AND project = ?2 AND (from_id = ?1 OR to_id = ?1)",
            params![id, project],
            |r| r.get(0),
        )
        .map_err(be)?;
    Ok(n as usize)
}

/// The 1-based line of the NEXT code-entity DEFINITION in `file` after `line`, in `project` (spec
/// 58) - the structural upper bound (exclusive) on a definition's body extent, since neither the
/// graph node nor the symbol index records a true end line. `None` when no later definition exists
/// in the file (the entity is the file's last definition). Filtered to real definitions (a `name`
/// attr present), matched by the file PREFIX of the id (`substr(id, 1, instr(id, '::') - 1)`), and
/// read directly off the persisted projection so a given tree + graph is deterministic.
fn next_definition_line(
    conn: &Connection,
    file: &str,
    line: u32,
    project: &str,
) -> Result<Option<u32>, Error> {
    let next: Option<i64> = conn
        .query_row(
            "SELECT MIN(CAST(json_extract(attrs, '$.line') AS INTEGER)) FROM nodes
              WHERE kind = ?1
                AND project = ?2
                AND json_extract(attrs, '$.name') IS NOT NULL
                AND substr(id, 1, instr(id, '::') - 1) = ?3
                AND CAST(json_extract(attrs, '$.line') AS INTEGER) > ?4",
            params![KIND_CODE_ENTITY, project, file, line],
            |r| r.get(0),
        )
        .map_err(be)?;
    Ok(next.map(|n| n as u32))
}

/// The confidence rank of an EDGE tier (spec 52 / 29a addendum 6.2): `extracted` (2, the precise
/// seed) > `inferred` (1, a derived cross-file link) > `ambiguous` (0, grep-visible-only). A
/// directed walk follows an edge only when its rank is at or above the floor.
fn tier_rank(tier: &str) -> u8 {
    match tier {
        TIER_EXTRACTED => 2,
        TIER_INFERRED => 1,
        _ => 0,
    }
}

/// The confidence rank of a requested tier FLOOR (spec 52). Unlike [`tier_rank`], an empty or
/// unrecognized value defaults to the resolvable floor (rank 1 = `inferred`): a directed walk
/// defaults to `extracted` + `inferred` and EXCLUDES the unresolved `ambiguous` tier, and a caller
/// opts the ambiguous tier in per-request by passing [`TIER_AMBIGUOUS`].
fn tier_floor_rank(tier: &str) -> u8 {
    match tier {
        TIER_EXTRACTED => 2,
        TIER_AMBIGUOUS => 0,
        _ => 1,
    }
}

/// Fetch a node's `(id, kind, attrs)` scoped to `project`, or `None` when it does not exist there
/// (spec 52). Read isolation matches [`Projection::subgraph`]: a same-id row in another project is
/// never returned. Used to seed the directed walk on real nodes only and to materialize each
/// reached node's kind/attrs for the result.
fn node_row(conn: &Connection, id: &str, project: &str) -> Result<Option<Node>, Error> {
    conn.query_row(
        "SELECT id, kind, attrs FROM nodes WHERE id = ?1 AND project = ?2",
        params![id, project],
        row_to_node,
    )
    .optional()
    .map_err(be)
}

/// Whether the node `id` carries a `name` attr in `project` (spec 52) - i.e. it is a code-entity
/// DEFINITION, not a bare cross-file placeholder the reference fold created attr-less. This is how
/// a directed hop tells a same-file callee (which already lands on its definition) from a bare
/// cross-file callee (which must be resolved by name-suffix).
fn node_has_name(conn: &Connection, id: &str, project: &str) -> Result<bool, Error> {
    let found = conn
        .query_row(
            "SELECT 1 FROM nodes
              WHERE id = ?1 AND project = ?2 AND json_extract(attrs, '$.name') IS NOT NULL",
            params![id, project],
            |_| Ok(()),
        )
        .optional()
        .map_err(be)?;
    Ok(found.is_some())
}

/// The code-entity DEFINITION nodes whose entity-name equals `suffix`, in `project`, sorted by id
/// (spec 52 - the conservative cross-file resolution's candidate set). Phrased with the PINNED
/// `substr(id, instr(id, '::') + 2)` expression so it seeks the `idx_nodes_name_suffix` expression
/// index rather than scanning; filtered to real definitions (a `name` attr present), so a bare
/// placeholder sharing the name is never itself a candidate.
fn definitions_with_suffix(
    conn: &Connection,
    suffix: &str,
    project: &str,
) -> Result<Vec<String>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM nodes
              WHERE substr(id, instr(id, '::') + 2) = ?1
                AND kind = ?2
                AND json_extract(attrs, '$.name') IS NOT NULL
                AND project = ?3
              ORDER BY id",
        )
        .map_err(be)?;
    let ids = stmt
        .query_map(params![suffix, KIND_CODE_ENTITY, project], |r| {
            r.get::<_, String>(0)
        })
        .map_err(be)?
        .collect::<Result<_, _>>()
        .map_err(be)?;
    Ok(ids)
}

/// The live, caller-attributed `CALLS` edges OUT of `from_id` in `project`, as
/// `(to_id, tier, valid_from, source)`, sorted by callee (spec 52). Carries the exact
/// `rel = CALLS AND valid_to IS NULL` shape the partial `idx_edges_live_rel_from` index serves, so
/// each hop's forward scan seeks rather than scans. The ordering makes the walk deterministic.
fn calls_out(
    conn: &Connection,
    from_id: &str,
    project: &str,
) -> Result<Vec<(String, String, i64, Position)>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT to_id, tier, valid_from, source FROM edges
              WHERE from_id = ?1 AND rel = ?2 AND valid_to IS NULL AND project = ?3
              ORDER BY to_id",
        )
        .map_err(be)?;
    let rows = stmt
        .query_map(params![from_id, REL_CALLS, project], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? as Position,
            ))
        })
        .map_err(be)?
        .collect::<Result<_, _>>()
        .map_err(be)?;
    Ok(rows)
}

/// Resolve one DOWN hop's callee target (spec 52 criterion 1). Returns the id the edge should land
/// on and, when the hop is a multi-candidate frontier, its SORTED candidate ids:
///
/// - a SAME-FILE callee already carries a `name` attr (it is its own definition) -> land on it,
///   no frontier;
/// - a BARE cross-file placeholder resolves by name-suffix over the DEFINITION nodes: EXACTLY ONE
///   definition auto-continues (the edge is redirected onto the real definition, no frontier);
///   MORE THAN ONE is a marked frontier (the placeholder id, carrying the sorted candidates) the
///   caller does NOT descend; ZERO leaves the placeholder a terminal leaf (no descent target).
fn resolve_down_hop(
    conn: &Connection,
    raw_to: &str,
    project: &str,
) -> Result<(String, Option<Vec<String>>), Error> {
    if node_has_name(conn, raw_to, project)? {
        return Ok((raw_to.to_string(), None));
    }
    let cands = definitions_with_suffix(conn, name_suffix(raw_to), project)?;
    match cands.len() {
        1 => Ok((cands.into_iter().next().unwrap(), None)),
        0 => Ok((raw_to.to_string(), None)),
        _ => Ok((raw_to.to_string(), Some(cands))),
    }
}

/// The live, caller-attributed `CALLS` edges whose target is EXACTLY `to_id`, in `project`, as
/// `(from_id, tier, valid_from, source)` sorted by caller (spec 52). These are the SAME-FILE callers
/// of `to_id`: a same-file call already lands on the callee's definition, so its edge literally
/// targets it - always an unambiguous caller. Carries the `rel = CALLS AND valid_to IS NULL` shape
/// the partial `idx_edges_live_rel_from` index cannot serve on `to_id`, but the reverse read stays
/// scoped and ordered so the UP walk is deterministic.
fn callers_direct(
    conn: &Connection,
    to_id: &str,
    project: &str,
) -> Result<Vec<(String, String, i64, Position)>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT from_id, tier, valid_from, source FROM edges
              WHERE to_id = ?1 AND rel = ?2 AND valid_to IS NULL AND project = ?3
              ORDER BY from_id",
        )
        .map_err(be)?;
    let rows = stmt
        .query_map(params![to_id, REL_CALLS, project], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? as Position,
            ))
        })
        .map_err(be)?
        .collect::<Result<_, _>>()
        .map_err(be)?;
    Ok(rows)
}

/// The live, caller-attributed `CALLS` edges into a BARE cross-file placeholder whose entity-name
/// equals `name`, in `project`, as `(from_id, tier, valid_from, source)` sorted by caller (spec 52 -
/// the reverse cross-file resolution). A cross-file call targets a bare placeholder in the CALLER's
/// file namespace (no `name` attr - the definition lives elsewhere), so these are the callers that
/// call `name` across a file boundary. Phrased with the PINNED `substr(id, instr(id,'::')+2)`
/// name-suffix expression so it seeks the `idx_nodes_name_suffix` index; filtered to bare targets (a
/// `name` attr absent) so an edge that lands directly on a real definition (a same-file caller, or a
/// call to a DIFFERENT same-named definition) is never miscounted here - those are covered by
/// [`callers_direct`] on the exact definition, keeping the two caller sets disjoint.
fn callers_via_bare(
    conn: &Connection,
    name: &str,
    project: &str,
) -> Result<Vec<(String, String, i64, Position)>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT e.from_id, e.tier, e.valid_from, e.source FROM edges e
               JOIN nodes n ON n.id = e.to_id AND n.project = e.project
              WHERE e.rel = ?1 AND e.valid_to IS NULL AND e.project = ?2
                AND substr(e.to_id, instr(e.to_id, '::') + 2) = ?3
                AND json_extract(n.attrs, '$.name') IS NULL
              ORDER BY e.from_id",
        )
        .map_err(be)?;
    let rows = stmt
        .query_map(params![REL_CALLS, project, name], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? as Position,
            ))
        })
        .map_err(be)?
        .collect::<Result<_, _>>()
        .map_err(be)?;
    Ok(rows)
}

/// One resolved UP caller hop (spec 52 criterion 3): `(caller_id, tier, valid_from, source,
/// frontier)`, where `frontier` is `Some(sorted candidate definition ids)` when the caller's
/// cross-file call is multi-candidate (the walk must not ascend it) and `None` for an unambiguous
/// caller. Named to keep the reverse-walk signatures out of clippy's `type_complexity`.
type CallerHop = (String, String, i64, Position, Option<Vec<String>>);

/// The callers of `cur` for one UP hop (spec 52 criterion 3), each a [`CallerHop`] whose `frontier`
/// carries the SORTED candidate definition ids on a multi-candidate hop the walk must NOT ascend.
/// Two disjoint sources, mirroring [`resolve_down_hop`] in reverse:
///
/// - **Same-file callers** ([`callers_direct`]) whose edge literally targets `cur` - always an
///   unambiguous caller, no frontier.
/// - **Cross-file callers** ([`callers_via_bare`]) that call `cur`'s NAME through a bare placeholder,
///   attributed to `cur` only when that name resolves unambiguously: EXACTLY ONE definition (`cur`
///   itself) -> a real caller, no frontier; MORE THAN ONE definition -> a marked frontier carrying
///   the sorted candidate ids (the caller might be calling a same-named sibling, so the walk stops
///   there). The cross-file source applies only when `cur` is a DEFINITION (carries a `name` attr):
///   a bare node is not a definition, so a cross-file call to its name resolves to the real
///   definitions elsewhere, never to it.
fn callers_of(conn: &Connection, cur: &str, project: &str) -> Result<Vec<CallerHop>, Error> {
    let mut out: Vec<CallerHop> = Vec::new();
    for (from, tier, vf, src) in callers_direct(conn, cur, project)? {
        out.push((from, tier, vf, src, None));
    }
    if node_has_name(conn, cur, project)? {
        let name = name_suffix(cur);
        let cands = definitions_with_suffix(conn, name, project)?;
        // `cur` is a definition of its own name, so it is always among the candidates: exactly one
        // means `cur` is the sole definition (unambiguous), more than one is a frontier.
        let frontier = if cands.len() > 1 { Some(cands) } else { None };
        for (from, tier, vf, src) in callers_via_bare(conn, name, project)? {
            out.push((from, tier, vf, src, frontier.clone()));
        }
    }
    Ok(out)
}

/// The FILE nodes that reference the seed's name(s) at file level but call them from no function
/// within them (spec 52 criterion 3 - the UP direction's "referenced but not called" sidecar).
/// Sorted by id, scoped to `project`.
///
/// A file references a name when it carries a live `REFERENCES` edge to a `<file>::<name>` target
/// whose entity-name matches; it CALLS the name when some caller inside it (a `<file>::<caller>`
/// source) carries a live `CALLS` edge to such a target. The result is the set difference -
/// files that reference but never call - the imports / uses a who-uses-this reader wants beside the
/// caller DAG. The seed names are the entity-name suffixes of the seed ids that exist as nodes; a
/// seed that is not a node contributes nothing. Empty when the seed has no such name (a bare or
/// missing seed).
fn referenced_not_called(
    conn: &Connection,
    seed: &[String],
    project: &str,
) -> Result<Vec<Node>, Error> {
    // The seed names to look up (entity-name suffixes of the seed ids that exist as nodes), deduped
    // and sorted for a deterministic query.
    let mut names: Vec<String> = Vec::new();
    for s in seed {
        if node_row(conn, s, project)?.is_some() {
            names.push(name_suffix(s).to_string());
        }
    }
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let names_json = serde_json::to_string(&names).map_err(be)?;

    // Files that reference any seed name at file level (a REFERENCES edge from the file container).
    let mut ref_stmt = conn
        .prepare(
            "SELECT DISTINCT from_id FROM edges
              WHERE rel = ?1 AND valid_to IS NULL AND project = ?2
                AND substr(to_id, instr(to_id, '::') + 2) IN (SELECT value FROM json_each(?3))",
        )
        .map_err(be)?;
    let referencing: Vec<String> = ref_stmt
        .query_map(params![REL_REFERENCES, project, names_json], |r| r.get(0))
        .map_err(be)?
        .collect::<Result<_, _>>()
        .map_err(be)?;

    // Files that CALL any seed name from within (the file part of each caller-attributed CALLS edge).
    let mut call_stmt = conn
        .prepare(
            "SELECT DISTINCT substr(from_id, 1, instr(from_id, '::') - 1) FROM edges
              WHERE rel = ?1 AND valid_to IS NULL AND project = ?2
                AND substr(to_id, instr(to_id, '::') + 2) IN (SELECT value FROM json_each(?3))",
        )
        .map_err(be)?;
    let calling: BTreeSet<String> = call_stmt
        .query_map(params![REL_CALLS, project, names_json], |r| r.get(0))
        .map_err(be)?
        .collect::<Result<_, _>>()
        .map_err(be)?;

    // Referenced but not called: materialize each such file as its node, sorted by id.
    let mut files: Vec<String> = referencing
        .into_iter()
        .filter(|f| !calling.contains(f))
        .collect();
    files.sort();
    files.dedup();
    let mut nodes: Vec<Node> = Vec::with_capacity(files.len());
    for f in files {
        if let Some(node) = node_row(conn, &f, project)? {
            nodes.push(node);
        }
    }
    Ok(nodes)
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
    // Machinery kinds/rels the fold no longer projects (spec 43 de-noise): imported here because
    // only these tests - which PROVE the machinery is gone - still name them. REL_RAISED stays in
    // the module-level import (invalidate_finding_edges still references it).
    use super::super::{
        KIND_AGENT, KIND_GATE, KIND_UNIT, META_ACTOR, REL_ASSIGNED_TO, REL_BLOCKS, REL_DECIDED,
        REL_GATED_BY, REL_TOUCHES,
    };

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
            .prune(&["drop-d".to_string(), "drop-f".to_string()], None)
            .unwrap();
        assert_eq!(removed.nodes, 2, "exactly the two named nodes are removed");
        assert_eq!(
            removed.superseded_edges, 0,
            "a node-only prune (None boundary) reclaims no superseded edges"
        );

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

    /// spec 49 criterion 2 (graph side): `apply_batch` folds a WHOLE batch in ONE call to the SAME
    /// graph that folding each event with `apply` would produce, and stays idempotent per position -
    /// so the batched-fold cadence changes only the transaction count, never the graph rows.
    #[test]
    fn apply_batch_folds_a_batch_equivalently_to_per_event_applies_and_is_idempotent() {
        let decision = |pos: u64, id: &str, path: &str| -> Event {
            let payload = serde_json::json!({ "id": id, "summary": "x", "governs": [path], "supersedes": "" });
            let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
            e.position = pos;
            e
        };
        let batch = vec![
            decision(1, "d1", "a.rs"),
            decision(2, "d2", "b.rs"),
            decision(3, "d3", "c.rs"),
        ];

        // Reference: fold each event one at a time.
        let per_event = Projector::open(":memory:", "test").unwrap();
        for e in &batch {
            per_event.apply(e).unwrap();
        }

        // Batched: fold the whole slice in ONE call.
        let batched = Projector::open(":memory:", "test").unwrap();
        batched.apply_batch(&batch).unwrap();

        assert_eq!(
            live_governs(&batched).len(),
            3,
            "apply_batch folds all three decisions' GOVERNS edges in one call"
        );
        assert_eq!(
            live_governs(&batched),
            live_governs(&per_event),
            "apply_batch yields the SAME graph as folding each event with apply"
        );

        // Idempotent per position: re-applying the SAME batch at the same positions adds nothing.
        batched.apply_batch(&batch).unwrap();
        assert_eq!(
            live_governs(&batched).len(),
            3,
            "re-applying a batch at the same positions must not double any edge"
        );
    }

    /// spec 49 criterion 2 (graph side): `apply_batch` is ONE transaction - a fold error partway
    /// through rolls the WHOLE batch back, so it is never half-applied. This distinguishes the
    /// single-transaction override from a per-event loop (which would have committed the earlier
    /// events before the later one failed), pinning the batched cadence itself, not just its result.
    #[test]
    fn apply_batch_is_atomic_a_mid_batch_fold_error_rolls_the_whole_batch_back() {
        // A good decision, then a POISON event: a DecisionMade whose data is not valid JSON, so its
        // fold errors AFTER the good event has already folded WITHIN the same transaction.
        let good = {
            let payload = serde_json::json!({
                "id": "d1", "summary": "x", "governs": ["a.rs"], "supersedes": ""
            });
            let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
            e.position = 1;
            e
        };
        let poison = {
            let mut e = Event::new(TYPE_DECISION_MADE, b"{ not valid json".to_vec());
            e.position = 2;
            e
        };

        let p = Projector::open(":memory:", "test").unwrap();
        assert!(
            p.apply_batch(&[good.clone(), poison]).is_err(),
            "a fold error must surface from apply_batch"
        );
        assert_eq!(
            live_governs(&p).len(),
            0,
            "the good event folded before the poison must be ROLLED BACK with the failed batch"
        );

        // The `applied` guard was rolled back too, so a retry re-folds the good event cleanly.
        p.apply(&good).unwrap();
        assert_eq!(
            live_governs(&p).len(),
            1,
            "after the rollback the good event still folds on its own (its position was not consumed)"
        );
    }

    /// Fold a `DecisionMade` (`id` GOVERNS `path`) from its raw on-log JSON at `pos`, with the
    /// event's valid-from set to `secs`. GOVERNS (decision -> file) is the SURVIVING content edge
    /// the spec-40 upsert-live dedup is demonstrated over: the fold no longer projects the old
    /// `agent --TOUCHES--> file` machinery edge (spec 43 de-noise), but `add_edge`'s collapse-a-
    /// re-assertion-into-the-one-live-edge behaviour is edge-agnostic, so a re-asserted
    /// decision->file GOVERNS edge exercises it exactly as a re-touch once did. `secs` sets the
    /// event's valid-from so a test can assert the collapsed edge keeps the EARLIEST assertion
    /// time; `pos` becomes the edge's `source`, so the LATEST assertion wins.
    fn apply_governs_at(p: &Projector, pos: u64, id: &str, path: &str, secs: u64) {
        let payload = serde_json::json!({
            "id": id, "summary": "x", "governs": [path], "supersedes": "",
        });
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap())
            .with_valid_from(UNIX_EPOCH + std::time::Duration::from_secs(secs));
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// Every LIVE `GOVERNS` edge as `(from, to, source, valid_from)`, read straight from the
    /// table (not through the live `subgraph` filter), so a test can COUNT the rows and prove a
    /// re-assertion collapsed into the one existing live edge rather than accreting a row per fold.
    fn live_governs(p: &Projector) -> Vec<(String, String, i64, i64)> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT from_id, to_id, source, valid_from FROM edges
                 WHERE rel = ?1 AND valid_to IS NULL ORDER BY from_id, to_id",
            )
            .unwrap();
        stmt.query_map(params![REL_GOVERNS], |r| {
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

    /// Every node as `(id, kind)`.
    type NodeKinds = Vec<(String, String)>;
    /// Every live edge as `(from, rel, to)`.
    type LiveEdges = Vec<(String, String, String)>;

    /// Every node's `(id, kind)` and every LIVE edge's `(from, rel, to)`, read straight from the
    /// tables. Unlike a seeded `subgraph`, this sees the WHOLE graph, so a test can prove a node
    /// kind or edge rel is ABSENT everywhere (a seeded neighborhood could only prove local absence).
    fn all_nodes_edges(p: &Projector) -> (NodeKinds, LiveEdges) {
        let conn = p.conn.lock().unwrap();
        let nodes = {
            let mut s = conn
                .prepare("SELECT id, kind FROM nodes ORDER BY id")
                .unwrap();
            s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let edges = {
            let mut s = conn
                .prepare(
                    "SELECT from_id, rel, to_id FROM edges
                     WHERE valid_to IS NULL ORDER BY from_id, rel, to_id",
                )
                .unwrap();
            s.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        };
        (nodes, edges)
    }

    #[test]
    fn re_asserted_governs_folds_to_one_live_edge_with_latest_provenance() {
        // Spec 40 criterion 1 (demonstrated over the surviving GOVERNS edge after the spec 43
        // de-noise dropped the old TOUCHES machinery vehicle): every re-fold of a decision that
        // GOVERNS a file re-asserts `decision --GOVERNS--> file`, and the old bare-insert fold
        // appended a fresh live row per assertion (measured worst case: 45 identical live rows for
        // a single relationship). The upsert-live fold collapses a re-assertion into the ONE
        // existing live edge, bumping its provenance to the LATEST assertion (source) and keeping
        // the EARLIEST valid_from - so N re-assertions yield exactly ONE live edge, while a
        // DIFFERENT decision or a DIFFERENT file still folds its own distinct live edge (dedup
        // removes only EXACT (from, rel, to, tier) duplicates).
        let p = Projector::open(":memory:", "test").unwrap();

        // d1 governs src/f.rs four times (positions 10..=13; valid_from 100..=400s).
        apply_governs_at(&p, 10, "d1", "src/f.rs", 100);
        apply_governs_at(&p, 11, "d1", "src/f.rs", 200);
        apply_governs_at(&p, 12, "d1", "src/f.rs", 300);
        apply_governs_at(&p, 13, "d1", "src/f.rs", 400);

        // A DIFFERENT decision and a DIFFERENT file each fold their own distinct live edge.
        apply_governs_at(&p, 14, "d2", "src/f.rs", 500);
        apply_governs_at(&p, 15, "d1", "src/g.rs", 600);

        let f = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(100));
        let g = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(600));
        let b = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(500));
        assert_eq!(
            live_governs(&p),
            vec![
                // d1->f collapsed from FOUR folds to ONE: source = latest (13), valid_from = earliest.
                ("d1".to_string(), "src/f.rs".to_string(), 13, f),
                // a different FILE is a distinct edge, untouched by the d1->f dedup.
                ("d1".to_string(), "src/g.rs".to_string(), 15, g),
                // a different DECISION is a distinct edge, untouched by the d1->f dedup.
                ("d2".to_string(), "src/f.rs".to_string(), 14, b),
            ],
            "N re-assertions of one relationship collapse to ONE live edge (source=latest, valid_from=earliest); a different decision/file keeps its own distinct live edge"
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

        // The canonical log the rebuild re-folds: decision d1 --GOVERNS--> src/f.rs re-asserted 45
        // times (the measured worst case - one fold per re-assertion), interleaved with two
        // DISTINCT relationships (a different decision, a different file) that must each survive the
        // rebuild as their own single live edge. GOVERNS is the surviving content edge the dedup is
        // demonstrated over after the spec 43 de-noise dropped the old TOUCHES machinery vehicle.
        let fold_log = |p: &Projector| {
            for pos in 1..=45u64 {
                apply_governs_at(p, pos, "d1", "src/f.rs", 100 * pos);
            }
            apply_governs_at(p, 46, "d2", "src/f.rs", 5000);
            apply_governs_at(p, 47, "d1", "src/g.rs", 6000);
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
                        "d1",
                        "src/f.rs",
                        REL_GOVERNS,
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
            live_governs(&dirty).len(),
            45,
            "premise: the old bare-insert fold left K=45 identical live rows for one relationship - the duplicates a rebuild must collapse"
        );

        // REBUILD - discard the dirty graph and fold the SAME log from scratch into a FRESH, EMPTY
        // projection. Every relationship collapses to exactly ONE live edge: the 45-fold d1
        // ->src/f.rs to a single row (source = latest position 45, valid_from = earliest), and the
        // two DISTINCT relationships each to their own single live edge.
        let rebuilt = Projector::open(":memory:", "test").unwrap();
        fold_log(&rebuilt);

        let f = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(100));
        let g = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(6000));
        let b = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(5000));
        let want = vec![
            // d1->src/f.rs: 45 duplicate live edges collapsed to ONE (source=45, valid_from=earliest).
            ("d1".to_string(), "src/f.rs".to_string(), 45, f),
            // a different FILE is a distinct relationship, its own single live edge.
            ("d1".to_string(), "src/g.rs".to_string(), 47, g),
            // a different DECISION is a distinct relationship, its own single live edge.
            ("d2".to_string(), "src/f.rs".to_string(), 46, b),
        ];
        assert_eq!(
            live_governs(&rebuilt),
            want,
            "a rebuild collapses the 45 duplicate live edges to exactly ONE per (from, rel, to, tier); distinct relationships each survive as their own single live edge"
        );

        // REBUILDABLE - a rebuild is a pure, reproducible function of the log: folding the SAME log
        // into ANOTHER fresh, empty projection re-derives the identical single-edge-per-key set.
        let rebuilt_again = Projector::open(":memory:", "test").unwrap();
        fold_log(&rebuilt_again);
        assert_eq!(
            live_governs(&rebuilt_again),
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

    /// A caller-attributed reference (spec 37): folds a `<file>::<caller> --CALLS--> <file>::<name>`
    /// edge alongside the file-level REFERENCES edge, so the community fixture builds real
    /// structural coupling the deterministic label is computed over.
    fn apply_ref_caller(p: &Projector, pos: u64, file: &str, name: &str, caller: &str) {
        let payload =
            serde_json::json!({ "file": file, "name": name, "lang": "rust", "caller": caller });
        let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// Construct and fold one `CommunityAssigned` event by hand (no detection-pass dependency), so
    /// the fold is proven in BOTH feature lanes exactly like the code-ingest fold tests.
    fn apply_community(
        p: &Projector,
        pos: u64,
        node: &str,
        community: &str,
        resolution: f64,
        hash: &str,
        fresh: bool,
    ) {
        let payload = serde_json::json!({
            "node": node, "community": community,
            "resolution": resolution, "hash": hash, "fresh": fresh,
        });
        let mut e = Event::new(
            TYPE_COMMUNITY_ASSIGNED,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = pos;
        p.apply(&e).unwrap();
    }

    /// Fold the canonical spec-53 community fixture: a coupling graph whose `apply_damage` hub is
    /// the clear highest-degree member, then a resolution-1.0 detection pass grouping members ACROSS
    /// directory lines (`src/combat`, `src/net`) into `community/1/0` and an equal-degree pair into
    /// `community/1/1` (its label decided by the lexicographic tie-break). Shared by the fold and
    /// the rebuild tests so they re-derive from the SAME log.
    fn seed_community_fixture(p: &Projector) {
        // Coupling graph (definitions).
        apply_code_entity(
            p,
            1,
            "src/combat/hit.rs",
            "apply_damage",
            "function",
            10,
            "rust",
        );
        apply_code_entity(p, 2, "src/combat/hit.rs", "clamp", "function", 30, "rust");
        apply_code_entity(p, 3, "src/net/socket.rs", "send", "function", 5, "rust");
        apply_code_entity(p, 4, "src/util.rs", "alpha", "function", 1, "rust");
        apply_code_entity(p, 5, "src/util.rs", "zeta", "function", 2, "rust");
        // `apply_damage` calls three symbols, making it the highest-degree hub (1 CONTAINS + 3
        // CALLS = degree 4); `clamp` reaches degree 3 (CONTAINS + the CALLS + the REFERENCES twin);
        // `send` stays at degree 1 (its CONTAINS only).
        apply_ref_caller(p, 6, "src/combat/hit.rs", "clamp", "apply_damage");
        apply_ref_caller(p, 7, "src/combat/hit.rs", "min", "apply_damage");
        apply_ref_caller(p, 8, "src/combat/hit.rs", "max", "apply_damage");
        // Detection pass at resolution 1.0. The FIRST event carries `fresh` (the pass boundary).
        apply_community(
            p,
            9,
            "src/combat/hit.rs::apply_damage",
            "community/1/0",
            1.0,
            "h-alpha",
            true,
        );
        apply_community(
            p,
            10,
            "src/combat/hit.rs::clamp",
            "community/1/0",
            1.0,
            "h-alpha",
            false,
        );
        apply_community(
            p,
            11,
            "src/net/socket.rs::send",
            "community/1/0",
            1.0,
            "h-alpha",
            false,
        );
        apply_community(
            p,
            12,
            "src/util.rs::alpha",
            "community/1/1",
            1.0,
            "h-alpha",
            false,
        );
        apply_community(
            p,
            13,
            "src/util.rs::zeta",
            "community/1/1",
            1.0,
            "h-alpha",
            false,
        );
    }

    /// A deterministic snapshot of the whole community layer: every KIND_COMMUNITY node (id, kind,
    /// attrs json) and every LIVE IN_COMMUNITY edge (from, to), each sorted. Two folds of the same
    /// log must produce byte-identical snapshots (the rebuildable-projection invariant).
    fn community_snapshot(p: &Projector) -> Vec<String> {
        let conn = p.conn.lock().unwrap();
        let mut rows: Vec<String> = Vec::new();
        let mut nstmt = conn
            .prepare(
                "SELECT id, kind, COALESCE(attrs, '') FROM nodes
                 WHERE kind = ?1 AND project = ?2 ORDER BY id",
            )
            .unwrap();
        rows.extend(
            nstmt
                .query_map(params![KIND_COMMUNITY, p.project], |r| {
                    Ok(format!(
                        "node\t{}\t{}\t{}",
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        );
        let mut estmt = conn
            .prepare(
                "SELECT from_id, to_id FROM edges
                 WHERE rel = ?1 AND valid_to IS NULL AND project = ?2
                 ORDER BY from_id, to_id",
            )
            .unwrap();
        rows.extend(
            estmt
                .query_map(params![REL_IN_COMMUNITY, p.project], |r| {
                    Ok(format!(
                        "edge\t{}\t{}",
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
        );
        rows
    }

    #[test]
    fn community_assigned_events_fold_to_a_community_node_with_live_membership_and_a_deterministic_label(
    ) {
        // Spec 53 criterion 3 (the EVENT-SOURCED FOLD): each CommunityAssigned event folds into a
        // live `<member> --IN_COMMUNITY--> <community>` edge PLUS the KIND_COMMUNITY super-node,
        // whose deterministic LABEL is its highest-degree member's label (ties broken to the
        // lexicographically-smallest). Built by hand (no detection dependency) so the fold is proven
        // in BOTH feature lanes. Owns the recording discipline; it does NOT own detection
        // (criterion 1) - the assignments are given.
        let p = Projector::open(":memory:", "test").unwrap();
        seed_community_fixture(&p);

        let g = p.whole().unwrap();

        // The community super-node folded, tagged KIND_COMMUNITY, carrying its resolution grain, the
        // pass hash, and a deterministic label - its highest-degree member (`apply_damage`, degree
        // 4), grouped ACROSS the src/combat and src/net directory lines.
        let comm = g
            .nodes
            .iter()
            .find(|n| n.id == "community/1/0")
            .expect("community super-node folded from CommunityAssigned");
        assert_eq!(comm.kind, KIND_COMMUNITY);
        assert_eq!(comm.attrs.get("resolution").map(String::as_str), Some("1"));
        assert_eq!(comm.attrs.get("hash").map(String::as_str), Some("h-alpha"));
        assert_eq!(
            comm.attrs.get("label").map(String::as_str),
            Some("apply_damage"),
            "the community label is its highest-degree member's label; got {:?}",
            comm.attrs
        );

        // Every member carries a LIVE membership edge to its community - across directories.
        for m in [
            "src/combat/hit.rs::apply_damage",
            "src/combat/hit.rs::clamp",
            "src/net/socket.rs::send",
        ] {
            assert!(
                g.edges.iter().any(|e| e.rel == REL_IN_COMMUNITY
                    && e.from == m
                    && e.to == "community/1/0"
                    && e.valid_to.is_none()),
                "a live IN_COMMUNITY edge ties member {m} to community/1/0; got {:?}",
                g.edges
            );
        }

        // A member with NO structural degree beyond its own container still folds; and the
        // equal-degree pair's community label resolves by the lexicographic tie-break.
        let comm1 = g
            .nodes
            .iter()
            .find(|n| n.id == "community/1/1")
            .expect("second community super-node folded");
        assert_eq!(
            comm1.attrs.get("label").map(String::as_str),
            Some("alpha"),
            "equal-degree members break the label tie to the lexicographically-smallest label; got {:?}",
            comm1.attrs
        );
    }

    #[test]
    fn a_rebuild_reproduces_byte_identical_community_rows_from_the_log() {
        // Spec 53 criterion 3: the community layer is a rebuildable projection of the log. Folding
        // the SAME log into a FRESH projection re-derives byte-identical community nodes (id, kind,
        // AND attrs - including the computed label) and live membership edges, so a full rebuild
        // reproduces the derivation without re-running detection.
        let p1 = Projector::open(":memory:", "test").unwrap();
        seed_community_fixture(&p1);
        let p2 = Projector::open(":memory:", "test").unwrap();
        seed_community_fixture(&p2);

        let s1 = community_snapshot(&p1);
        let s2 = community_snapshot(&p2);
        assert!(
            s1.iter().any(|r| r.starts_with("node\t"))
                && s1.iter().any(|r| r.starts_with("edge\t")),
            "precondition: the fixture folded community nodes and membership edges; got {s1:?}"
        );
        assert_eq!(
            s1, s2,
            "a rebuild from the same log re-derives byte-identical community rows"
        );
    }

    #[test]
    fn a_fresh_rerun_supersedes_only_its_own_resolution_grain() {
        // Spec 53 criterion 3 (the fold's supersession mechanism; criterion 2 owns the grain
        // semantics): a re-run's `fresh` boundary retires ONLY the re-run resolution's prior
        // memberships, leaving a DIFFERENT resolution grain's memberships live and coexisting.
        let p = Projector::open(":memory:", "test").unwrap();
        apply_code_entity(&p, 1, "a.rs", "x", "function", 1, "rust");
        apply_code_entity(&p, 2, "b.rs", "y", "function", 1, "rust");
        // Resolution-1.0 pass: x, y in community/1/0.
        apply_community(&p, 3, "a.rs::x", "community/1/0", 1.0, "h1", true);
        apply_community(&p, 4, "b.rs::y", "community/1/0", 1.0, "h1", false);
        // Resolution-2.0 pass (a DIFFERENT grain): x in community/2/0.
        apply_community(&p, 5, "a.rs::x", "community/2/0", 2.0, "h2", true);
        // Re-run resolution 1.0 (fresh): x moves to community/1/1.
        apply_community(&p, 6, "a.rs::x", "community/1/1", 1.0, "h1b", true);
        apply_community(&p, 7, "b.rs::y", "community/1/1", 1.0, "h1b", false);

        let x_memberships: Vec<(String, Option<i64>)> = edges_from(&p, "a.rs::x")
            .into_iter()
            .filter(|(_to, rel, _vt)| rel == REL_IN_COMMUNITY)
            .map(|(to, _rel, vt)| (to, vt))
            .collect();

        // The prior resolution-1.0 membership is superseded (valid_to set), never deleted.
        assert!(
            x_memberships
                .iter()
                .any(|(to, vt)| to == "community/1/0" && vt.is_some()),
            "the re-run supersedes x's prior community/1/0 membership (valid_to set); got {x_memberships:?}"
        );
        // Exactly ONE live resolution-1.0 membership remains: the new community/1/1 assignment.
        let live_res1: Vec<&String> = x_memberships
            .iter()
            .filter(|(to, vt)| to.starts_with("community/1/") && vt.is_none())
            .map(|(to, _)| to)
            .collect();
        assert_eq!(
            live_res1,
            vec![&"community/1/1".to_string()],
            "the re-run leaves exactly one live resolution-1.0 membership, the new grain; got {x_memberships:?}"
        );
        // The resolution-2.0 grain is UNTOUCHED by the resolution-1.0 re-run - it stays live.
        assert!(
            x_memberships
                .iter()
                .any(|(to, vt)| to == "community/2/0" && vt.is_none()),
            "the resolution-2.0 membership stays live across a resolution-1.0 re-run; got {x_memberships:?}"
        );
    }

    #[test]
    fn a_concept_rerun_supersedes_only_its_grain_through_the_real_derivation_pipeline() {
        // Spec 54 RE-RUN SUPERSESSION (this unit OWNS the concept lifecycle claim; the community
        // sibling `a_fresh_rerun_supersedes_only_its_own_resolution_grain` proves the mirror over
        // hand-built CommunityAssigned events). Unlike that sibling - and unlike the c1 fold-periphery
        // tests that hand-build events - this drives the REAL concepts pipeline end to end
        // (`intent_layer` -> `derive` -> `events` -> fold) over a CHANGED intent layer at the SAME
        // resolution, so the concept arm's edge-retire is LOAD-BEARING: a member MOVES from
        // concept/1/1 to concept/1/0, and only the retire keeps it from ending with two live r=1
        // memberships.
        //
        // Mutation teeth, TWO independent mechanisms (this unit's diff is test-only; production stays
        // byte-identical):
        //   (a) RETIRE FIRES (d-u54c4-teeth-mutation-proven): neutralize the concept arm's
        //       `if c.fresh { UPDATE edges SET valid_to ... }` (e.g. its WHERE -> `WHERE 1=0`) and this
        //       reddens for the RIGHT reason - the mover then shows TWO live r=1 memberships
        //       [concept/1/0, concept/1/1] and no retired concept/1/1 row.
        //   (b) PREFIX BOUNDARY (d-u54c4-rerun2-adjacent-prefix-isolation): the retire scopes by
        //       `format!("concept/{res}/")` at sqlite.rs:1495, and the TRAILING SLASH is the sole guard
        //       that an r=1 re-run (prefix "concept/1/") does not bleed into ADJACENT-numeric grains
        //       (concept/10/*, concept/11/*). Drop that slash (prefix -> "concept/1") and
        //       `substr("concept/10/0",1,9) == "concept/1"` matches, so the re-run retires the r=10/r=11
        //       grains' edges AND drops their now-memberless concept nodes - reddening the byte-identical
        //       isolation assertions below. This is the ISOLATION half of the d54-c4 charter this unit
        //       OWNS ("leaving other resolutions untouched"); it mirrors the community sibling
        //       `a_firing_rerun_supersedes_only_its_own_resolution_grain` (community/1/ vs community/10/).
        //       A NON-adjacent control (r=2, concept/2/*) alone is VACUOUS here: "concept/1" never
        //       prefix-matches "concept/2" whether or not the slash is present.
        use crate::concepts::{derive, events, intent_layer, Derivation};

        // Two DISJOINT intent regions, each a design doc SPECIFYING its own files. Region A's doc has
        // the lexicographically-smallest id, so it is `concept/1/0`; region B's doc is `concept/1/1`
        // (groups are numbered by ascending representative). `b2_in_a` rewires ONE file (`src/b2.rs`)
        // from region B's doc onto region A's doc - how the mover crosses grains at the SAME
        // resolution: a genuine re-derivation, not a relabelled event.
        fn two_region_intent(b2_in_a: bool) -> Graph {
            let doc = |id: &str, title: &str| {
                let mut attrs = BTreeMap::new();
                attrs.insert("title".to_string(), title.to_string());
                Node {
                    id: id.to_string(),
                    kind: KIND_DESIGN_DOC.to_string(),
                    attrs,
                }
            };
            let file = |id: &str| Node {
                id: id.to_string(),
                kind: KIND_FILE.to_string(),
                attrs: BTreeMap::new(),
            };
            let spec = |from: &str, to: &str| Edge {
                from: from.to_string(),
                to: to.to_string(),
                rel: REL_SPECIFIES.to_string(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: TIER_EXTRACTED.to_string(),
            };
            let nodes = vec![
                doc("docs/a.md", "Region A"),
                doc("docs/b.md", "Region B"),
                file("src/a1.rs"),
                file("src/a2.rs"),
                file("src/b1.rs"),
                file("src/b2.rs"),
            ];
            let mut edges = vec![
                spec("docs/a.md", "src/a1.rs"),
                spec("docs/a.md", "src/a2.rs"),
                spec("docs/b.md", "src/b1.rs"),
            ];
            // The mover: originally region B's doc specifies it; after the change region A's doc does.
            edges.push(spec(
                if b2_in_a { "docs/a.md" } else { "docs/b.md" },
                "src/b2.rs",
            ));
            Graph { nodes, edges }
        }

        // Stamp a pass's events with unique ascending positions (fold idempotency is per-position) and
        // ONE increasing valid_from per pass, so the re-run's retire stamps a valid_to that strictly
        // post-dates the edges it retires (a clean bi-temporal order, not merely a non-null marker).
        fn stamped(evs: Vec<Event>, base_pos: u64, secs: u64) -> Vec<Event> {
            evs.into_iter()
                .enumerate()
                .map(|(i, mut e)| {
                    e.position = base_pos + i as u64;
                    e.valid_from = UNIX_EPOCH + std::time::Duration::from_secs(secs);
                    e
                })
                .collect()
        }

        let membership =
            |d: &Derivation| -> BTreeMap<String, String> { d.members.iter().cloned().collect() };

        // Original r=1 derivation: the mover starts in concept/1/1 (region B).
        let g1 = two_region_intent(false);
        let d1 = derive(&g1, &intent_layer(&g1), 1.0);
        let m1 = membership(&d1);
        assert_eq!(
            m1["docs/a.md"], "concept/1/0",
            "region A's doc is concept/1/0"
        );
        assert_eq!(
            m1["docs/b.md"], "concept/1/1",
            "region B's doc is concept/1/1"
        );
        assert_eq!(
            m1["src/b2.rs"], "concept/1/1",
            "the mover starts in concept/1/1"
        );

        // Coexisting isolation controls over the SAME original layer, each a DIFFERENT resolution grain
        // the r=1 re-run must leave byte-identical. `format!("{res}")` renders 2.0/10.0/11.0 as
        // "2"/"10"/"11" (matching the id-grain segment the retire scopes by), so:
        //   - r=2 (concept/2/*): the NON-adjacent numeric control. "concept/1" never prefix-matches
        //     "concept/2", so this holds whether or not the retire prefix carries its trailing slash -
        //     the WEAK guarantee (per-resolution scoping in general, NOT the boundary this unit owns).
        //   - r=10 and r=11 (concept/10/*, concept/11/*): the ADJACENT-numeric controls. These are the
        //     LOAD-BEARING proof of the retire's trailing-slash prefix boundary (see mutation teeth (b)
        //     on this test): drop the slash and "concept/1" bleeds into concept/10//concept/11/,
        //     reddening their assertions below.
        let d2 = derive(&g1, &intent_layer(&g1), 2.0);
        let d10 = derive(&g1, &intent_layer(&g1), 10.0);
        let d11 = derive(&g1, &intent_layer(&g1), 11.0);
        for (res, d) in [(2.0_f64, &d2), (10.0, &d10), (11.0, &d11)] {
            assert!(
                !events(d).is_empty(),
                "the r={res} isolation grain is non-empty, so its events fire"
            );
        }

        let p = Projector::open(":memory:", "test").unwrap();
        p.apply_batch(&stamped(events(&d1), 1, 1_000)).unwrap();
        p.apply_batch(&stamped(events(&d2), 1_000, 1_500)).unwrap();
        p.apply_batch(&stamped(events(&d10), 2_000, 1_600)).unwrap();
        p.apply_batch(&stamped(events(&d11), 3_000, 1_700)).unwrap();

        // A grain's REALIZES edges across its member nodes, read RAW (retired rows included) and keyed
        // by the grain's own `concept/<res>/` id prefix - the exact substring the retire scopes by.
        let grain_realizes = |p: &Projector,
                              members: &[(String, String)],
                              prefix: &str|
         -> Vec<(String, String, Option<i64>)> {
            let mut rows: Vec<(String, String, Option<i64>)> = members
                .iter()
                .map(|(node, _)| node.clone())
                .collect::<BTreeSet<String>>()
                .into_iter()
                .flat_map(|node| {
                    edges_from(p, &node)
                        .into_iter()
                        .filter(|(to, rel, _)| rel == REL_REALIZES && to.starts_with(prefix))
                        .map(move |(to, _rel, vt)| (node.clone(), to, vt))
                })
                .collect();
            rows.sort();
            rows
        };
        // The grain's LIVE concept super-nodes. The retire's node-drop half (sqlite.rs:1513 DELETE FROM
        // nodes) is scoped by the SAME `concept/<res>/` prefix, so the boundary must hold for nodes too
        // - dropping the slash would DELETE the concept/10//concept/11/ super-nodes once their edges
        // bleed-retire. Mirrors the community sibling's node-side check (`grain_community_nodes`).
        let grain_concept_nodes = |p: &Projector, prefix: &str| -> Vec<String> {
            let mut ids: Vec<String> = p
                .whole()
                .unwrap()
                .nodes
                .into_iter()
                .filter(|n| n.kind == KIND_CONCEPT && n.id.starts_with(prefix))
                .map(|n| n.id)
                .collect();
            ids.sort();
            ids
        };

        let r2_before = grain_realizes(&p, &d2.members, "concept/2/");
        let r10_edges_before = grain_realizes(&p, &d10.members, "concept/10/");
        let r10_nodes_before = grain_concept_nodes(&p, "concept/10/");
        let r11_edges_before = grain_realizes(&p, &d11.members, "concept/11/");
        let r11_nodes_before = grain_concept_nodes(&p, "concept/11/");
        for (what, empty) in [
            ("r=2 edges", r2_before.is_empty()),
            ("r=10 edges", r10_edges_before.is_empty()),
            ("r=10 concept nodes", r10_nodes_before.is_empty()),
            ("r=11 edges", r11_edges_before.is_empty()),
            ("r=11 concept nodes", r11_nodes_before.is_empty()),
        ] {
            assert!(
                !empty,
                "precondition: the {what} isolation grain folded a non-empty live layer \
                 (else the boundary guard below is vacuous)"
            );
        }

        // Re-derive r=1 over the CHANGED layer (the fresh boundary): the mover crosses to concept/1/0.
        let g2 = two_region_intent(true);
        let d1b = derive(&g2, &intent_layer(&g2), 1.0);
        let m1b = membership(&d1b);
        assert_eq!(
            m1b["src/b2.rs"], "concept/1/0",
            "after the change the mover derives into concept/1/0"
        );
        p.apply_batch(&stamped(events(&d1b), 4_000, 3_000)).unwrap();

        // === RE-RUN SUPERSESSION, asserted on the mover via the edges_from RAW read ===
        let mover: Vec<(String, Option<i64>)> = edges_from(&p, "src/b2.rs")
            .into_iter()
            .filter(|(_to, rel, _)| rel == REL_REALIZES)
            .map(|(to, _rel, vt)| (to, vt))
            .collect();

        // The prior concept/1/1 membership is RETIRED (valid_to set) - kept in the table, not deleted.
        assert!(
            mover
                .iter()
                .any(|(to, vt)| to == "concept/1/1" && vt.is_some()),
            "the mover's prior concept/1/1 membership is retired (valid_to set), not deleted; got {mover:?}"
        );
        assert!(
            !mover
                .iter()
                .any(|(to, vt)| to == "concept/1/1" && vt.is_none()),
            "no live concept/1/1 membership survives for the mover; got {mover:?}"
        );
        // Exactly ONE live r=1 membership remains: the new concept/1/0 grain.
        let live_r1: Vec<&String> = mover
            .iter()
            .filter(|(to, vt)| to.starts_with("concept/1/") && vt.is_none())
            .map(|(to, _)| to)
            .collect();
        assert_eq!(
            live_r1,
            vec![&"concept/1/0".to_string()],
            "the re-run leaves the mover exactly one live r=1 membership, the new concept/1/0; got {mover:?}"
        );

        // === ISOLATION: every OTHER resolution grain is byte-identical across the r=1 re-run ===
        // The NON-adjacent control (holds regardless of the trailing slash - the weak guarantee).
        assert_eq!(
            grain_realizes(&p, &d2.members, "concept/2/"),
            r2_before,
            "the resolution-2.0 grain stays byte-identical across a resolution-1.0 re-run"
        );
        // The ADJACENT-numeric controls - the LOAD-BEARING proof of the retire's trailing-slash prefix
        // boundary (mutation teeth (b)): both the edge retire (sqlite.rs:1495) AND the node drop
        // (sqlite.rs:1513) must exclude concept/10//concept/11/. Dropping the slash reddens these.
        assert_eq!(
            grain_realizes(&p, &d10.members, "concept/10/"),
            r10_edges_before,
            "the ADJACENT concept/10/ grain's REALIZES edges are untouched by the r=1 re-run - the \
             `concept/1/` retire prefix's trailing slash excludes concept/10/ (drop the slash and \
             this reddens)"
        );
        assert_eq!(
            grain_concept_nodes(&p, "concept/10/"),
            r10_nodes_before,
            "the ADJACENT concept/10/ grain's concept super-nodes survive the r=1 re-run - the \
             node-drop half of the retire also respects the trailing-slash boundary"
        );
        assert_eq!(
            grain_realizes(&p, &d11.members, "concept/11/"),
            r11_edges_before,
            "the ADJACENT concept/11/ grain's REALIZES edges are untouched by the r=1 re-run (the \
             same trailing-slash boundary, one adjacent grain further out)"
        );
        assert_eq!(
            grain_concept_nodes(&p, "concept/11/"),
            r11_nodes_before,
            "the ADJACENT concept/11/ grain's concept super-nodes survive the r=1 re-run"
        );
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

    /// spec 58 criterion 1: `locate` is the show surface's single resolution authority. Over the
    /// SAME node/edge tables every graph surface reads, it resolves a full `<file>::<name>` id and a
    /// unique bare name to the SAME site (carrying kind, one-hop degree, and the next-definition
    /// extent bound), LISTS the SORTED candidates for an ambiguous bare name (never guessing), and
    /// returns `None` for an unknown query. The code-entity fold it reads is always compiled, so
    /// this holds in BOTH feature lanes.
    #[test]
    fn locate_resolves_by_id_and_name_and_lists_ambiguous_candidates_sorted() {
        let p = Projector::open(":memory:", "test").unwrap();
        // a.rs defines alpha@1 then shared@3; b.rs defines a second shared@1.
        apply_code_entity(&p, 1, "a.rs", "alpha", "function", 1, "rust");
        apply_code_entity(&p, 2, "a.rs", "shared", "function", 3, "rust");
        apply_code_entity(&p, 3, "b.rs", "shared", "function", 1, "rust");

        // A unique bare name resolves to One, with its site facts.
        let alpha = match p.locate("alpha").unwrap() {
            Located::One(s) => s,
            other => panic!("expected One for a unique name, got {other:?}"),
        };
        assert_eq!(alpha.id, "a.rs::alpha");
        assert_eq!(alpha.file, "a.rs");
        assert_eq!(alpha.line, 1);
        assert_eq!(alpha.kind, "function");
        // The only edge incident to alpha is the file's CONTAINS edge -> one-hop degree 1.
        assert_eq!(alpha.degree, 1);
        // The next definition in a.rs is shared@3, so alpha's body extent is bounded at line 3.
        assert_eq!(alpha.next_def_line, Some(3));

        // The full id resolves to the SAME entity a unique name would.
        match p.locate("a.rs::alpha").unwrap() {
            Located::One(s) => assert_eq!(s, alpha, "the full id and the unique name agree"),
            other => panic!("expected One for a full id, got {other:?}"),
        }

        // shared@a.rs is the file's LAST definition, so it has no next-definition bound.
        match p.locate("a.rs::shared").unwrap() {
            Located::One(s) => assert_eq!(s.next_def_line, None),
            other => panic!("expected One for a.rs::shared, got {other:?}"),
        }

        // An ambiguous bare name lists its candidates, SORTED by id, each with its file.
        match p.locate("shared").unwrap() {
            Located::Many(cands) => {
                let ids: Vec<&str> = cands.iter().map(|c| c.id.as_str()).collect();
                assert_eq!(
                    ids,
                    ["a.rs::shared", "b.rs::shared"],
                    "candidates are sorted by id"
                );
                assert_eq!(cands[0].file, "a.rs");
                assert_eq!(cands[1].file, "b.rs");
            }
            other => panic!("expected Many for an ambiguous name, got {other:?}"),
        }

        // An unknown query resolves to None (never an error).
        assert_eq!(p.locate("does_not_exist").unwrap(), Located::None);
    }

    #[test]
    fn whole_reads_the_full_projection_excludes_invalidated_edges_and_scopes_by_project() {
        // Spec 45, criterion 2: `whole()` is the DIRECT read the `/api/graph` provider consults -
        // the entire live projection, NO seed and NO reachability walk. It must (a) return every
        // node and every currently-valid edge, (b) EXCLUDE an edge invalidated by a supersede
        // (`valid_to` set), (c) be scoped to its own project on a shared backend, and (d) sort
        // deterministically.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();

        // Project p1: d1 governs a.rs, then d2 supersedes d1 and ALSO governs a.rs (so d1's GOVERNS
        // edge is invalidated while d2's stays live), plus a code entity in b.rs.
        let p1 = Projector::open(path, "p1").unwrap();
        apply_decision(&p1, 1, "d1", "old", &["a.rs"], "");
        apply_decision(&p1, 2, "d2", "new", &["a.rs"], "d1");
        apply_code_entity(&p1, 3, "b.rs", "run", "function", 1, "rust");

        // Project p2 on the SAME backend file: its nodes must never leak into p1's whole read.
        let p2 = Projector::open(path, "p2").unwrap();
        apply_decision(&p2, 1, "pz", "other-project", &["z.rs"], "");

        let g = p1.whole().unwrap();

        // (a) every p1 node is present - both decisions, the governed file, the code entity and its
        // file container - WITHOUT any seed.
        for id in ["d1", "d2", "a.rs", "b.rs", "b.rs::run"] {
            assert!(
                g.nodes.iter().any(|n| n.id == id),
                "whole() returns node {id}, got {:?}",
                g.nodes
            );
        }
        // (c) p2's nodes are NOT visible through p1's scoped whole read.
        assert!(
            !g.nodes.iter().any(|n| n.id == "pz" || n.id == "z.rs"),
            "whole() is project-scoped: p2 nodes never leak into p1, got {:?}",
            g.nodes
        );

        // (a) the live GOVERNS edge (from d2) and the live structural CONTAINS edge are returned.
        assert!(
            g.edges
                .iter()
                .any(|e| e.from == "d2" && e.to == "a.rs" && e.rel == REL_GOVERNS),
            "the live GOVERNS edge is returned, got {:?}",
            g.edges
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.from == "b.rs" && e.to == "b.rs::run" && e.rel == REL_CONTAINS),
            "the live CONTAINS edge is returned, got {:?}",
            g.edges
        );
        // (b) the supersede-invalidated GOVERNS edge (from d1) is EXCLUDED by the valid_to filter.
        assert!(
            !g.edges
                .iter()
                .any(|e| e.from == "d1" && e.rel == REL_GOVERNS),
            "the supersede-invalidated GOVERNS edge is excluded, got {:?}",
            g.edges
        );

        // (d) deterministic: nodes come back sorted by id.
        let ids: Vec<String> = g.nodes.iter().map(|n| n.id.clone()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(
            ids, sorted,
            "whole() returns nodes sorted by id, got {ids:?}"
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

    /// Like [`apply_batch_def`] but stamps the event's `valid_from` at `secs` past the epoch, so a
    /// test can place each extraction batch in a distinct run and predict the `valid_to` a later
    /// batch's supersession writes (`to_nanos(valid_from)`) - the retention boundary spec 41 keys on.
    fn apply_batch_def_at(
        p: &Projector,
        pos: u64,
        file: &str,
        name: &str,
        line: u32,
        fresh: bool,
        secs: u64,
    ) {
        let payload = serde_json::json!({
            "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
            "fresh": fresh,
        });
        let mut e = Event::new(
            TYPE_CODE_ENTITY_EXTRACTED,
            serde_json::to_vec(&payload).unwrap(),
        )
        .with_valid_from(UNIX_EPOCH + std::time::Duration::from_secs(secs));
        e.position = pos;
        p.apply(&e).unwrap();
    }

    #[test]
    fn prune_reclaims_superseded_structural_edges_older_than_the_boundary_and_keeps_every_live_edge(
    ) {
        // Spec 41 criterion 1 (OWNS the superseded-edge prune): a file re-extracted across MULTIPLE
        // runs accretes one superseded structural edge per prior extraction - the `fresh` batch
        // boundary sets `valid_to` on the file's prior live edges before folding the new batch. The
        // extended prune authority reclaims ONLY the superseded rows (`valid_to IS NOT NULL`) retired
        // BEFORE the retention boundary (the active run's start), while EVERY live edge
        // (`valid_to IS NULL`) remains and a row retired AT the boundary is kept as recent history -
        // so the historical tail is bounded, but LIVE is sacrosanct and the log re-derives the rest.
        let p = Projector::open(":memory:", "test").unwrap();
        let file = "src/a.rs";

        // Run 1 (t=100s): first extraction of `foo` and `bar` - two live CONTAINS edges (vf=100s).
        apply_batch_def_at(&p, 1, file, "foo", 5, true, 100);
        apply_batch_def_at(&p, 2, file, "bar", 9, false, 100);

        // Run 2 (t=200s): re-extract `foo` only (bar deleted). The fresh event supersedes run-1's
        // CONTAINS(foo)+CONTAINS(bar) with valid_to=to_nanos(200s), then folds a new live CONTAINS(foo).
        apply_batch_def_at(&p, 10, file, "foo", 12, true, 200);

        // Run 3 (t=300s, the ACTIVE run): re-extract `foo` and add `baz`. This supersedes run-2's
        // CONTAINS(foo) with valid_to=to_nanos(300s) and folds live CONTAINS(foo)+CONTAINS(baz).
        apply_batch_def_at(&p, 20, file, "foo", 3, true, 300);
        apply_batch_def_at(&p, 21, file, "baz", 7, false, 300);

        let boundary = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(300));

        // Before: three superseded rows (run-1 foo@200s, run-1 bar@200s, run-2 foo@300s) and two
        // live rows (run-3 foo, run-3 baz) out of the file node.
        let before = edges_from(&p, file);
        let live_before: Vec<_> = before
            .iter()
            .filter(|(_, _, vt)| vt.is_none())
            .cloned()
            .collect();
        let superseded_before = before.iter().filter(|(_, _, vt)| vt.is_some()).count();
        assert_eq!(
            superseded_before, 3,
            "three superseded structural edges accrued across the re-extractions; got {before:?}"
        );
        assert_eq!(
            live_before.len(),
            2,
            "the active run's two live CONTAINS edges; got {before:?}"
        );

        // Prune with NO dead-run nodes - only the spec-41 superseded-edge reclamation at the
        // active-run boundary.
        let stats = p.prune(&[], Some(boundary)).unwrap();
        assert_eq!(
            stats.superseded_edges, 2,
            "exactly the two rows retired BEFORE the boundary (run-1 foo+bar, valid_to=200s) are reclaimed"
        );
        assert_eq!(stats.nodes, 0, "no node was named to drop");

        // After: the two run-1 rows (valid_to=200s < boundary) are gone; the run-2 row
        // (valid_to=300s == boundary) survives as recent history; BOTH live rows (valid_to NULL)
        // are untouched - LIVE is sacrosanct.
        let after = edges_from(&p, file);
        let superseded_after: Vec<_> = after
            .iter()
            .filter(|(_, _, vt)| vt.is_some())
            .cloned()
            .collect();
        let live_after: Vec<_> = after
            .iter()
            .filter(|(_, _, vt)| vt.is_none())
            .cloned()
            .collect();
        assert_eq!(
            superseded_after.len(),
            1,
            "only the boundary-time (recent) superseded row survives; got {after:?}"
        );
        assert!(
            superseded_after
                .iter()
                .all(|(_, _, vt)| *vt == Some(boundary)),
            "the surviving superseded row is exactly the one retired AT the boundary; got {after:?}"
        );
        assert_eq!(
            live_before, live_after,
            "EVERY live edge is untouched by the prune (LIVE is sacrosanct)"
        );
    }

    /// A live `subgraph` result reduced to a sorted, comparable form: nodes as `(id, kind, attrs)`
    /// and edges as `(from, rel, to, tier, valid_from, source)`. `subgraph` returns ONLY live rows
    /// (`valid_to IS NULL`), so this is exactly the slice a grounding consumer reads; the
    /// superseded-edge prune must leave it identical, because it reclaims only historical rows a
    /// live query never sees. Sorted so the comparison is order-independent (SQLite returns rows in
    /// no guaranteed order).
    #[allow(clippy::type_complexity)]
    fn live_slice(
        g: &Graph,
    ) -> (
        Vec<(String, String, BTreeMap<String, String>)>,
        Vec<(String, String, String, String, i64, Position)>,
    ) {
        let mut nodes: Vec<_> = g
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.kind.clone(), n.attrs.clone()))
            .collect();
        nodes.sort();
        let mut edges: Vec<_> = g
            .edges
            .iter()
            .map(|e| {
                (
                    e.from.clone(),
                    e.rel.clone(),
                    e.to.clone(),
                    e.tier.clone(),
                    e.valid_from,
                    e.source,
                )
            })
            .collect();
        edges.sort();
        (nodes, edges)
    }

    #[test]
    fn pruning_superseded_edges_leaves_the_live_grounding_subgraph_exactly_unchanged() {
        // Spec 41 criterion 2 (OWNS the live-invariant guarantee): the superseded-edge prune reclaims
        // ONLY historical rows (`valid_to IS NOT NULL`), so the LIVE slice a grounding consumer reads
        // through `subgraph` (which filters `valid_to IS NULL`) is byte-for-byte identical before and
        // after. This is the dedicated grounding-unaffected proof - a RICH live slice (two files;
        // CONTAINS / REFERENCES / caller-attributed CALLS) over a table carrying MANY superseded rows,
        // pruned NON-vacuously, returns the exact same nodes AND edges, and no live edge is removed at
        // the storage layer either. It does NOT own the prune mechanism's exact reclamation semantics
        // (criterion 1) or the bounded-growth regression (criterion 3) - only that LIVE is sacrosanct,
        // so no grounding, blast-radius, or safe-superset consumer loses a reference it needs.
        let p = Projector::open(":memory:", "test").unwrap();
        let a = "src/a.rs";
        let b = "src/b.rs";

        // Project-scoped raw edge counts, straight from the private table, so the LIVE-invariant is
        // proven at the storage layer INCLUDING the caller-attributed CALLS rows (from an entity, not
        // the file, so `edges_from(a)` alone would miss them).
        let count_live = |proj: &Projector| -> i64 {
            let conn = proj.conn.lock().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM edges WHERE valid_to IS NULL AND project = ?1",
                params![proj.project],
                |r| r.get(0),
            )
            .unwrap()
        };
        let count_superseded = |proj: &Projector| -> i64 {
            let conn = proj.conn.lock().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM edges WHERE valid_to IS NOT NULL AND project = ?1",
                params![proj.project],
                |r| r.get(0),
            )
            .unwrap()
        };

        // a.rs is re-extracted across THREE runs, so the edges table accretes a superseded copy of its
        // prior structural edges on every re-extraction - historical rows the live view never shows.
        // b.rs is extracted ONCE and never re-extracted, contributing purely-live structure the prune
        // must equally leave untouched.
        //
        // Run 1 (t=100s): a.rs defines foo + bar and CALLS helper from inside foo; b.rs defines gadget
        // and CALLS widget from inside gadget.
        apply_batch_def_at(&p, 1, a, "foo", 5, true, 100);
        apply_batch_def_at(&p, 2, a, "bar", 9, false, 100);
        apply_batch_ref_caller(&p, 3, a, "helper", "foo");
        apply_batch_def_at(&p, 4, b, "gadget", 2, true, 100);
        apply_batch_ref_caller(&p, 5, b, "widget", "gadget");

        // Run 2 (t=200s): re-extract a.rs (bar deleted). The fresh event supersedes run-1's a.rs edges
        // with valid_to=to_nanos(200s), then folds a.rs's new live structure.
        apply_batch_def_at(&p, 10, a, "foo", 12, true, 200);
        apply_batch_ref_caller(&p, 11, a, "helper", "foo");

        // Run 3 (t=300s, the ACTIVE run): re-extract a.rs adding baz. Supersedes run-2's a.rs edges
        // with valid_to=to_nanos(300s); folds a.rs's final live structure (foo, baz, helper).
        apply_batch_def_at(&p, 20, a, "foo", 3, true, 300);
        apply_batch_def_at(&p, 21, a, "baz", 7, false, 300);
        apply_batch_ref_caller(&p, 22, a, "helper", "foo");

        let boundary = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(300));

        // Precondition (non-vacuity): the table genuinely carries superseded rows retired BEFORE the
        // boundary - without them the prune is a no-op and the invariance vacuous.
        let superseded_pre = count_superseded(&p);
        assert!(
            superseded_pre > 0,
            "precondition: re-extraction accrued superseded historical rows to reclaim; got {superseded_pre}"
        );
        let live_pre = count_live(&p);

        // The LIVE slice a grounding consumer reads, captured over BOTH files at a depth spanning the
        // whole live structure, BEFORE the prune.
        let seed = [a.to_string(), b.to_string()];
        let before = live_slice(&p.subgraph(&seed, 3).unwrap());
        assert!(
            !before.0.is_empty() && !before.1.is_empty(),
            "the captured live slice is non-empty - real nodes and edges to protect; got {before:?}"
        );

        // Prune the superseded edges at the active-run boundary (no dead-run nodes). This reclaims
        // historical rows the live view never shows - and it MUST reclaim some, or the invariance
        // below is proven only against a no-op.
        let stats: PruneStats = p.prune(&[], Some(boundary)).unwrap();
        assert!(
            stats.superseded_edges > 0,
            "the prune reclaimed historical rows (non-vacuous); got {stats:?}"
        );
        assert_eq!(
            stats.nodes, 0,
            "no node was named to drop - only the superseded-edge reclamation ran"
        );

        // THE criterion-2 proof: the LIVE slice a grounding consumer reads is EXACTLY the same after
        // the prune - the same nodes AND the same edges. Grounding, blast-radius, and the two-view
        // safe superset are unaffected because only historical rows were reclaimed.
        let after = live_slice(&p.subgraph(&seed, 3).unwrap());
        assert_eq!(
            before, after,
            "the live subgraph a grounding consumer reads is byte-for-byte unchanged by the prune"
        );

        // The LIVE-invariant at the storage layer: NOT ONE live edge was removed (a live edge outside
        // the seeded slice would escape the subgraph check above but not this project-wide count),
        // while the superseded tail genuinely shrank - the prune removed ONLY historical rows.
        assert_eq!(
            count_live(&p),
            live_pre,
            "no live edge is removed by the prune - the live edge count is unchanged (LIVE is sacrosanct)"
        );
        assert!(
            count_superseded(&p) < superseded_pre,
            "the superseded historical tail genuinely shrank; before={superseded_pre} after={}",
            count_superseded(&p)
        );
    }

    #[test]
    fn pruning_keeps_the_superseded_edge_count_bounded_by_the_window_across_n_re_extractions() {
        // Spec 41 criterion 3 (OWNS the bounded-growth regression). The measured failure was the
        // superseded-edge table growing WITHOUT bound as runs re-extract: every re-extraction's
        // `fresh` batch boundary retires the file's prior live structural edges (spec 29a), and
        // before spec 41 nothing reclaimed them - 301,156 superseded rows piled up, ~90% of the
        // table and 70MB of an 83MB graph, slow enough on every fold to stall a loop run. This
        // proves the fix BOUNDS that growth: re-extracting the SAME file N times accretes O(N)
        // superseded rows, but a prune at the active-run boundary leaves a count fixed by the
        // RETENTION WINDOW (the most recent run's supersessions), NOT growing with N. It is proven
        // for two Ns an order of magnitude apart - the pre-prune tail scales with N while the
        // post-prune count is IDENTICAL - so a regression that stopped reclaiming would make the two
        // post-prune counts diverge and fail here.
        //
        // Scope is strictly the O(N) bound: the LIVE-untouched / subgraph-equivalence guarantee is
        // criterion 2's to own, and the prune mechanism itself is criterion 1's. The single
        // live-count assertion here is a guard on this test's premise, not the owned criterion.

        // Re-extract a file with a STABLE two-definition shape (foo + bar) across `runs` runs, run r
        // at t=100*r s. Run r's FIRST event is `fresh`, so it retires run (r-1)'s two live CONTAINS
        // edges, stamping their valid_to = to_nanos(100*r), then the batch re-folds the two live
        // edges. Prune at the ACTIVE run's (run `runs`) boundary and return the superseded/live row
        // counts read straight from the edge table, before and after the prune.
        let experiment = |runs: u64| -> (usize, usize, usize, usize) {
            let p = Projector::open(":memory:", "test").unwrap();
            let file = "src/a.rs";
            let mut pos = 0u64;
            for r in 1..=runs {
                let secs = 100 * r;
                pos += 1;
                apply_batch_def_at(&p, pos, file, "foo", 5, true, secs); // fresh: retires prior live
                pos += 1;
                apply_batch_def_at(&p, pos, file, "bar", 9, false, secs);
            }
            // Count the file's outgoing CONTAINS rows by liveness, straight from the table (the live
            // `subgraph` filter hides the superseded ones this bound is about).
            let superseded = |p: &Projector| {
                edges_from(p, file)
                    .into_iter()
                    .filter(|(_, _, vt)| vt.is_some())
                    .count()
            };
            let live = |p: &Projector| {
                edges_from(p, file)
                    .into_iter()
                    .filter(|(_, _, vt)| vt.is_none())
                    .count()
            };
            let superseded_before = superseded(&p);
            let live_before = live(&p);
            // The active run's start is the retention boundary (the same run-boundary attribution
            // `reset --runs` uses): reclaim only superseded rows retired BEFORE it.
            let boundary = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(100 * runs));
            let stats = p.prune(&[], Some(boundary)).unwrap();
            let superseded_after = superseded(&p);
            let live_after = live(&p);
            // The prune's reported reclamation equals the rows it actually removed from the table.
            assert_eq!(
                stats.superseded_edges,
                superseded_before - superseded_after,
                "prune's reported superseded_edges equals the superseded rows it removed (runs={runs})"
            );
            (superseded_before, superseded_after, live_before, live_after)
        };

        let (before_5, after_5, live_before_5, live_after_5) = experiment(5);
        let (before_50, after_50, live_before_50, live_after_50) = experiment(50);

        // UNBOUNDED WITHOUT THE PRUNE: the superseded tail grows O(N) with re-extractions - each of
        // the (runs - 1) supersessions retires the stable 2-edge shape, so before any prune the
        // count is 2*(runs - 1), and an order-of-magnitude-larger N carries an order-of-magnitude-
        // larger tail (the unbounded accumulation spec 41 measured).
        assert_eq!(
            before_5,
            2 * (5 - 1),
            "5 re-extractions accrue 2*(5-1) superseded rows"
        );
        assert_eq!(
            before_50,
            2 * (50 - 1),
            "50 re-extractions accrue 2*(50-1) superseded rows"
        );
        assert!(
            before_50 > before_5,
            "the pre-prune superseded tail GROWS with N: {before_50} (N=50) > {before_5} (N=5)"
        );

        // BOUNDED WITH THE PRUNE: after the active-run-boundary prune, the superseded count is fixed
        // by the retention window - exactly the most recent run's supersessions (the stable 2-edge
        // shape, retired AT the boundary and so kept as recent history) - independent of N. The
        // count is IDENTICAL for N=5 and N=50: the regression guard, since a reclamation that
        // stopped would leave O(N) rows and the two counts would diverge.
        assert_eq!(
            after_5, 2,
            "after the prune only the last run's 2 supersessions (the window) remain, not O(N)"
        );
        assert_eq!(
            after_50, after_5,
            "the post-prune superseded count is bounded by the window - IDENTICAL for N=50 and N=5, not O(N): {after_50} vs {after_5}"
        );

        // Premise guard (criterion 2 OWNS the full live-invariant): the active run's live shape is
        // unchanged by the prune and independent of N - the prune reclaimed history only.
        assert_eq!(
            (live_before_5, live_after_5, live_before_50, live_after_50),
            (2, 2, 2, 2),
            "the prune leaves the active run's live edges untouched (a guard; criterion 2 owns this)"
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
    fn calls_down_walks_the_execution_path_as_a_layered_deduped_dag_with_a_back_edge() {
        // Spec 52 criterion 1: the DOWN traversal. From a seed with a SAME-FILE callee and a
        // SINGLE-CANDIDATE cross-file callee, `calls(Down)` returns the transitive DAG with correct
        // per-node LAYERS, DEDUPED nodes under a cycle, and the recursive edge marked as a BACK
        // edge. A cross-file hop resolves THROUGH its bare placeholder to the real definition and
        // continues; nothing is a frontier (every hop here is single-candidate).
        let p = Projector::open(":memory:", "test").unwrap();
        let a = "src/a.rs";
        let b = "src/b.rs";

        // Definitions first (so the cross-file references fold INFERRED, not AMBIGUOUS): a.rs
        // defines `main` and `helper`; b.rs defines `work`.
        apply_batch_def(&p, 1, a, "main", 1, true);
        apply_batch_def(&p, 2, a, "helper", 5, false);
        apply_batch_def(&p, 3, b, "work", 1, true);
        // Calls: main -> helper (same-file), main -> work (single-candidate cross-file), and
        // work -> main (cross-file, closing a CYCLE back onto the seed).
        apply_batch_ref_caller(&p, 4, a, "helper", "main");
        apply_batch_ref_caller(&p, 5, a, "work", "main");
        apply_batch_ref_caller(&p, 6, b, "main", "work");

        let node_ids = |cg: &CallGraph| -> Vec<String> {
            let mut v: Vec<String> = cg.nodes.iter().map(|n| n.node.id.clone()).collect();
            v.sort();
            v
        };
        let edge_pairs = |cg: &CallGraph| -> Vec<(String, String)> {
            let mut v: Vec<(String, String)> = cg
                .edges
                .iter()
                .map(|e| (e.edge.from.clone(), e.edge.to.clone()))
                .collect();
            v.sort();
            v
        };

        let cg = p
            .calls(
                &["src/a.rs::main".to_string()],
                Direction::Down,
                5,
                TIER_INFERRED,
            )
            .unwrap();

        // LAYERS: the seed at 0; both its callees at 1; the cross-file callee resolved to its real
        // definition b.rs::work (NOT the bare a.rs::work placeholder). The cycle back onto main
        // DEDUPS - main appears EXACTLY ONCE, still at layer 0.
        let layer = |id: &str| -> Option<i64> {
            cg.nodes.iter().find(|n| n.node.id == id).map(|n| n.layer)
        };
        assert_eq!(
            layer("src/a.rs::main"),
            Some(0),
            "the seed is layer 0; nodes were {:?}",
            node_ids(&cg)
        );
        assert_eq!(
            layer("src/a.rs::helper"),
            Some(1),
            "the same-file callee is layer 1; nodes were {:?}",
            node_ids(&cg)
        );
        assert_eq!(
            layer("src/b.rs::work"),
            Some(1),
            "the cross-file callee resolved to its definition at layer 1; nodes were {:?}",
            node_ids(&cg)
        );
        assert_eq!(
            cg.nodes
                .iter()
                .filter(|n| n.node.id == "src/a.rs::main")
                .count(),
            1,
            "the recursion dedups: main appears exactly once (a DAG, not a loop); nodes were {:?}",
            node_ids(&cg)
        );
        // The walk resolved THROUGH the bare cross-file placeholders - they are not in the answer.
        for bare in ["src/a.rs::work", "src/b.rs::main"] {
            assert!(
                !cg.nodes.iter().any(|n| n.node.id == bare),
                "the bare cross-file placeholder {bare} is resolved away, not returned; nodes were {:?}",
                node_ids(&cg)
            );
        }
        assert_eq!(
            node_ids(&cg),
            vec!["src/a.rs::helper", "src/a.rs::main", "src/b.rs::work"],
            "exactly the seed plus its two resolved callees"
        );

        // No FRONTIER: every hop is single-candidate, so no node is a marked frontier.
        assert!(
            cg.nodes.iter().all(|n| n.frontier.is_none()),
            "single-candidate hops carry no frontier marker; nodes were {:?}",
            cg.nodes
                .iter()
                .map(|n| (n.node.id.clone(), n.frontier.clone()))
                .collect::<Vec<_>>()
        );

        // EDGES: two forward tree edges (back=false) and the recursion edge work -> main (back=true).
        let back_of = |from: &str, to: &str| -> Option<bool> {
            cg.edges
                .iter()
                .find(|e| e.edge.from == from && e.edge.to == to)
                .map(|e| e.back)
        };
        assert_eq!(
            back_of("src/a.rs::main", "src/a.rs::helper"),
            Some(false),
            "the same-file forward edge is not a back edge; edges were {:?}",
            edge_pairs(&cg)
        );
        assert_eq!(
            back_of("src/a.rs::main", "src/b.rs::work"),
            Some(false),
            "the resolved cross-file forward edge lands on the definition, not a back edge; edges were {:?}",
            edge_pairs(&cg)
        );
        assert_eq!(
            back_of("src/b.rs::work", "src/a.rs::main"),
            Some(true),
            "the recursion edge closing the cycle onto the seed is marked BACK; edges were {:?}",
            edge_pairs(&cg)
        );
        assert_eq!(
            edge_pairs(&cg),
            vec![
                ("src/a.rs::main".to_string(), "src/a.rs::helper".to_string()),
                ("src/a.rs::main".to_string(), "src/b.rs::work".to_string()),
                ("src/b.rs::work".to_string(), "src/a.rs::main".to_string()),
            ],
            "exactly the three resolved CALLS edges"
        );
        assert!(
            cg.edges.iter().all(|e| e.edge.rel == REL_CALLS),
            "every traversed edge is a CALLS edge"
        );
    }

    #[test]
    fn calls_down_follows_a_single_candidate_hop_but_marks_a_multi_candidate_one_a_sorted_frontier()
    {
        // Spec 52 criterion 2 - the CONSERVATIVE RESOLUTION policy, hardened past c1's coverage
        // (adj-u52cdw-c2-scope-reconcile). One seed reaches BOTH kinds of cross-file hop, so a single
        // walk proves the boundary the policy turns on:
        //   - a SINGLE-definition callee IS followed - resolved through its bare placeholder onto the
        //     real definition, and the walk DESCENDS past it (its own callee is reached one layer
        //     deeper);
        //   - a MULTI-definition callee becomes a marked FRONTIER carrying its SORTED candidate ids
        //     and is NOT descended (honest by construction - the view may be INCOMPLETE but never
        //     confidently wrong; the human re-seeds on a chosen candidate).
        // The candidate-sort assertion has TEETH the c1 periphery test lacks: that test folds a name's
        // two definitions a.rs-then-b.rs, so their natural row order already EQUALS the sorted order
        // and the sort is unproven (sdet-u52cdw-frontier-sort-vacuous). Here `dup`'s b.rs definition
        // is folded BEFORE its a.rs one, so the natural (rowid) row order is [b, a]; only the
        // `ORDER BY id` in `definitions_with_suffix` yields the asserted [a, b] - drop the sort and
        // this assertion fails.
        let p = Projector::open(":memory:", "test").unwrap();

        // Definitions, folded before the calls so every cross-file reference tiers INFERRED (a
        // definition already exists), placing it at/above the default floor.
        apply_batch_def(&p, 1, "src/caller.rs", "entry", 1, true); // the seed
        apply_batch_def(&p, 2, "src/solo.rs", "solo", 1, true); // the SINGLE-candidate callee
        apply_batch_def(&p, 3, "src/sink.rs", "sink", 1, true); // solo's own callee (proves descent)
                                                                // `dup` is defined in TWO files - fold b.rs BEFORE a.rs so the pre-sort (rowid) order is
                                                                // [b, a] and only `ORDER BY id` re-sorts it to [a, b].
        apply_batch_def(&p, 4, "src/b.rs", "dup", 1, true);
        apply_batch_def(&p, 5, "src/b.rs", "only_via_b", 2, false);
        apply_batch_def(&p, 6, "src/a.rs", "dup", 1, true);
        apply_batch_def(&p, 7, "src/a.rs", "only_via_a", 2, false);

        // Calls: entry -> solo (single-candidate cross-file) and entry -> dup (multi-candidate);
        // solo -> sink (so the followed single-candidate hop has somewhere to descend); and each
        // `dup` candidate calls a distinct sentinel that is reachable ONLY by descending that
        // candidate.
        apply_batch_ref_caller(&p, 8, "src/caller.rs", "solo", "entry");
        apply_batch_ref_caller(&p, 9, "src/caller.rs", "dup", "entry");
        apply_batch_ref_caller(&p, 10, "src/solo.rs", "sink", "solo");
        apply_batch_ref_caller(&p, 11, "src/a.rs", "only_via_a", "dup");
        apply_batch_ref_caller(&p, 12, "src/b.rs", "only_via_b", "dup");

        let cg = p
            .calls(
                &["src/caller.rs::entry".to_string()],
                Direction::Down,
                5,
                TIER_INFERRED,
            )
            .unwrap();

        let node_ids = |cg: &CallGraph| -> Vec<String> {
            let mut v: Vec<String> = cg.nodes.iter().map(|n| n.node.id.clone()).collect();
            v.sort();
            v
        };
        let node = |id: &str| cg.nodes.iter().find(|n| n.node.id == id);

        // SINGLE-candidate: `solo` IS followed - resolved onto its real definition (not the bare
        // caller-namespace placeholder), at layer 1, carrying NO frontier marker.
        let solo = node("src/solo.rs::solo")
            .expect("the single-candidate callee is followed onto its def");
        assert_eq!(
            solo.layer, 1,
            "the followed single-candidate callee is one hop from the seed"
        );
        assert!(
            solo.frontier.is_none(),
            "a single-candidate hop is fully resolved - not a frontier"
        );
        assert!(
            node("src/caller.rs::solo").is_none(),
            "the bare cross-file placeholder is resolved away, not returned; nodes were {:?}",
            node_ids(&cg),
        );
        // ...and the walk DESCENDS past it: solo's own callee is reached at layer 2, so the
        // single-candidate hop was followed THROUGH, not merely landed on.
        let sink =
            node("src/sink.rs::sink").expect("the walk descends past the followed single hop");
        assert_eq!(
            sink.layer, 2,
            "solo's callee is two hops from the seed - the single-candidate hop was traversed"
        );

        // MULTI-candidate: `dup` is a marked FRONTIER on the bare placeholder, carrying its SORTED
        // candidate ids. TEETH: b.rs was folded before a.rs, so a missing `ORDER BY id` would return
        // [b, a]; only the real sort yields [a, b].
        let dup = node("src/caller.rs::dup")
            .expect("the multi-candidate callee is returned as a frontier node");
        assert_eq!(dup.layer, 1, "the frontier callee is one hop from the seed");
        assert_eq!(
            dup.frontier,
            Some(vec![
                "src/a.rs::dup".to_string(),
                "src/b.rs::dup".to_string()
            ]),
            "the frontier carries its candidate ids SORTED by id - even though b.rs was folded first",
        );

        // The multi-candidate hop is the ONLY frontier; the single-candidate hop did not become one.
        assert_eq!(
            cg.nodes.iter().filter(|n| n.frontier.is_some()).count(),
            1,
            "exactly one node is a frontier - the multi-candidate hop, not the single one",
        );

        // NOT DESCENDED: neither candidate definition, nor anything reachable ONLY through a
        // candidate, appears - the walk stopped at the frontier and never guessed.
        for hidden in [
            "src/a.rs::dup",
            "src/b.rs::dup",
            "src/a.rs::only_via_a",
            "src/b.rs::only_via_b",
        ] {
            assert!(
                node(hidden).is_none(),
                "{hidden} lies beyond the un-descended frontier and must not be reached; nodes were {:?}",
                node_ids(&cg),
            );
        }

        // Exactly the seed, the followed single-candidate chain, and the marked frontier placeholder.
        assert_eq!(
            node_ids(&cg),
            vec![
                "src/caller.rs::dup".to_string(),
                "src/caller.rs::entry".to_string(),
                "src/sink.rs::sink".to_string(),
                "src/solo.rs::solo".to_string(),
            ],
            "the reached set is the seed + the followed single-candidate chain + the marked frontier",
        );
    }

    #[test]
    fn calls_down_leaves_a_zero_candidate_cross_file_callee_a_terminal_leaf_not_a_frontier() {
        // Spec 52 criterion 2 - the ZERO-candidate branch of the resolution policy
        // (adj-u52cdw-c2-scope-reconcile). A bare cross-file callee whose name is defined NOWHERE the
        // graph knows resolves to ZERO candidates: `resolve_down_hop` returns the bare placeholder id
        // itself with NO frontier - it is a terminal LEAF, categorically distinct from a
        // multi-candidate frontier (there is nothing to fan out to). Such a call tiers `ambiguous`,
        // so the floor is lowered to reach it; the point proved here is the resolution OUTCOME (a
        // bare leaf, `frontier` None), not the floor itself.
        let p = Projector::open(":memory:", "test").unwrap();
        apply_batch_def(&p, 1, "src/caller.rs", "entry", 1, true);
        // `ghost` is defined in no file the graph knows - the CALLS edge tiers ambiguous and, once
        // followed, resolves to zero candidates.
        apply_batch_ref_caller(&p, 2, "src/caller.rs", "ghost", "entry");

        let cg = p
            .calls(
                &["src/caller.rs::entry".to_string()],
                Direction::Down,
                5,
                TIER_AMBIGUOUS,
            )
            .unwrap();

        let ghost = cg
            .nodes
            .iter()
            .find(|n| n.node.id == "src/caller.rs::ghost")
            .expect("the zero-candidate callee is reached on the bare placeholder itself");
        assert_eq!(
            ghost.layer, 1,
            "the unresolved callee sits one hop from the seed"
        );
        assert!(
            ghost.frontier.is_none(),
            "a ZERO-candidate resolution is a terminal leaf, NOT a frontier (nothing to fan out to)",
        );
        // It is a genuine LEAF: nothing descends from the bare placeholder.
        assert!(
            !cg.edges
                .iter()
                .any(|e| e.edge.from == "src/caller.rs::ghost"),
            "no edge leaves the zero-candidate leaf; edges were {:?}",
            cg.edges
                .iter()
                .map(|e| (e.edge.from.clone(), e.edge.to.clone()))
                .collect::<Vec<_>>(),
        );
        // Exactly the seed and its one terminal leaf - the walk neither errored nor guessed.
        let mut ids: Vec<String> = cg.nodes.iter().map(|n| n.node.id.clone()).collect();
        ids.sort();
        assert_eq!(
            ids,
            vec![
                "src/caller.rs::entry".to_string(),
                "src/caller.rs::ghost".to_string()
            ],
            "the walk resolves the unknown callee to a bare terminal leaf, nothing more",
        );
    }

    #[test]
    fn calls_up_walks_the_call_sites_as_a_layered_deduped_dag_and_lists_referenced_but_not_called()
    {
        // Spec 52 criterion 3: the UP traversal. From a seed DEFINITION, `calls(Up)` returns the
        // transitive CALLER DAG - callers resolving THROUGH bare cross-file placeholders back onto the
        // seed's definition (the reverse name-match), each at its correct per-node LAYER, deduped
        // under a mutual-call cycle whose recursive edge is marked BACK - PLUS the flat, non-traversed
        // "referenced but not called" list of files that import/use the seed name without calling it.
        let p = Projector::open(":memory:", "test").unwrap();
        let a = "src/a.rs";
        let b = "src/b.rs";
        let c = "src/c.rs";
        let d = "src/d.rs";

        // Definitions first, so every cross-file reference tiers INFERRED (a definition already
        // exists), placing it at/above the default floor. a.rs defines the SEED `target` and a
        // same-file caller `local`; b.rs defines `mid`; c.rs defines `top`.
        apply_batch_def(&p, 1, a, "target", 1, true);
        apply_batch_def(&p, 2, a, "local", 2, false);
        apply_batch_def(&p, 3, b, "mid", 1, true);
        apply_batch_def(&p, 4, c, "top", 1, true);
        // Calls (each is a caller-attributed reference the emit pass produces for a call in a body):
        //   local  -> target  (SAME-FILE: lands directly on the seed def)
        //   mid    -> target  (CROSS-FILE single-candidate: through the bare b.rs::target placeholder)
        //   top    -> mid     (CROSS-FILE single-candidate: through the bare c.rs::mid placeholder)
        //   target -> mid     (the mutual call that closes a CYCLE - target both defines the seed and
        //                      calls mid, so walking UP from target reaches mid, whose callers include
        //                      target again: a BACK edge, deduped, not re-ascended)
        apply_batch_ref_caller(&p, 5, a, "target", "local");
        apply_batch_ref_caller(&p, 6, a, "mid", "target");
        apply_batch_ref_caller(&p, 7, b, "target", "mid");
        apply_batch_ref_caller(&p, 8, c, "mid", "top");
        // d.rs imports/uses `target` at TOP LEVEL (no enclosing caller): a file-level REFERENCES edge
        // with NO CALLS twin - the "referenced but not called" site.
        apply_batch_ref(&p, 9, d, "target", true);

        let node_ids = |cg: &CallGraph| -> Vec<String> {
            let mut v: Vec<String> = cg.nodes.iter().map(|n| n.node.id.clone()).collect();
            v.sort();
            v
        };
        let edge_pairs = |cg: &CallGraph| -> Vec<(String, String)> {
            let mut v: Vec<(String, String)> = cg
                .edges
                .iter()
                .map(|e| (e.edge.from.clone(), e.edge.to.clone()))
                .collect();
            v.sort();
            v
        };

        let cg = p
            .calls(
                &["src/a.rs::target".to_string()],
                Direction::Up,
                5,
                TIER_INFERRED,
            )
            .unwrap();

        // LAYERS: the seed at 0; its direct same-file caller and its single-candidate cross-file
        // caller at 1; the caller-of-a-caller at 2. Cross-file callers resolved THROUGH their bare
        // placeholders onto the real caller definitions (b.rs::mid, c.rs::top), never the bare nodes.
        let layer = |id: &str| -> Option<i64> {
            cg.nodes.iter().find(|n| n.node.id == id).map(|n| n.layer)
        };
        assert_eq!(
            layer("src/a.rs::target"),
            Some(0),
            "the seed is layer 0; nodes were {:?}",
            node_ids(&cg)
        );
        assert_eq!(
            layer("src/a.rs::local"),
            Some(1),
            "the same-file caller is layer 1; nodes were {:?}",
            node_ids(&cg)
        );
        assert_eq!(
            layer("src/b.rs::mid"),
            Some(1),
            "the cross-file caller resolved to its def at layer 1; nodes were {:?}",
            node_ids(&cg)
        );
        assert_eq!(
            layer("src/c.rs::top"),
            Some(2),
            "the caller-of-a-caller is two hops up; nodes were {:?}",
            node_ids(&cg)
        );
        // The bare cross-file placeholders the callers literally target are resolved away.
        for bare in ["src/b.rs::target", "src/c.rs::mid", "src/a.rs::mid"] {
            assert!(
                !cg.nodes.iter().any(|n| n.node.id == bare),
                "the bare cross-file placeholder {bare} is resolved away, not returned; nodes were {:?}",
                node_ids(&cg),
            );
        }
        // The mutual call dedups: the seed appears EXACTLY ONCE, still at layer 0 (a DAG, not a loop).
        assert_eq!(
            cg.nodes
                .iter()
                .filter(|n| n.node.id == "src/a.rs::target")
                .count(),
            1,
            "the recursion dedups: the seed appears exactly once; nodes were {:?}",
            node_ids(&cg),
        );
        assert_eq!(
            node_ids(&cg),
            vec![
                "src/a.rs::local".to_string(),
                "src/a.rs::target".to_string(),
                "src/b.rs::mid".to_string(),
                "src/c.rs::top".to_string(),
            ],
            "exactly the seed plus its transitive resolved callers",
        );

        // No FRONTIER: every caller's call is single-candidate here.
        assert!(
            cg.nodes.iter().all(|n| n.frontier.is_none()),
            "single-candidate caller hops carry no frontier marker; nodes were {:?}",
            cg.nodes
                .iter()
                .map(|n| (n.node.id.clone(), n.frontier.clone()))
                .collect::<Vec<_>>(),
        );

        // EDGES keep the real CALLS direction (caller -> callee). Three forward caller edges and the
        // mutual recursion target -> mid marked BACK (its caller, the seed, sits no deeper than mid).
        let back_of = |from: &str, to: &str| -> Option<bool> {
            cg.edges
                .iter()
                .find(|e| e.edge.from == from && e.edge.to == to)
                .map(|e| e.back)
        };
        assert_eq!(
            back_of("src/a.rs::local", "src/a.rs::target"),
            Some(false),
            "the same-file forward caller edge is not a back edge; edges were {:?}",
            edge_pairs(&cg)
        );
        assert_eq!(back_of("src/b.rs::mid", "src/a.rs::target"), Some(false), "the resolved cross-file caller edge lands on the seed def, not a back edge; edges were {:?}", edge_pairs(&cg));
        assert_eq!(
            back_of("src/c.rs::top", "src/b.rs::mid"),
            Some(false),
            "the caller-of-a-caller forward edge is not a back edge; edges were {:?}",
            edge_pairs(&cg)
        );
        assert_eq!(
            back_of("src/a.rs::target", "src/b.rs::mid"),
            Some(true),
            "the mutual-call edge closing the cycle is marked BACK; edges were {:?}",
            edge_pairs(&cg)
        );
        assert_eq!(
            edge_pairs(&cg),
            vec![
                (
                    "src/a.rs::local".to_string(),
                    "src/a.rs::target".to_string()
                ),
                ("src/a.rs::target".to_string(), "src/b.rs::mid".to_string()),
                ("src/b.rs::mid".to_string(), "src/a.rs::target".to_string()),
                ("src/c.rs::top".to_string(), "src/b.rs::mid".to_string()),
            ],
            "exactly the four resolved caller edges",
        );
        assert!(
            cg.edges.iter().all(|e| e.edge.rel == REL_CALLS),
            "every traversed edge is a CALLS edge"
        );

        // REFERENCED-BUT-NOT-CALLED: d.rs imports `target` without calling it, so it is the one
        // non-traversed site. a.rs and b.rs both CALL target (local, mid), so - though they reference
        // it - they are callers in the DAG, never in this list; and it is a FILE node, not an entity.
        let refd: Vec<String> = cg
            .referenced_not_called
            .iter()
            .map(|n| n.id.clone())
            .collect();
        assert_eq!(
            refd,
            vec!["src/d.rs".to_string()],
            "only the import-only file is referenced-but-not-called (callers are excluded)",
        );
        assert_eq!(
            cg.referenced_not_called[0].kind, KIND_FILE,
            "the referenced-but-not-called entry is the referencing FILE node",
        );
    }

    #[test]
    fn calls_up_marks_an_ambiguous_cross_file_caller_a_frontier_and_does_not_ascend_it() {
        // Spec 52 criterion 3, the honest-by-construction half of the UP walk (the reverse of the
        // DOWN conservative-resolution policy). A cross-file caller calls a name with MORE THAN ONE
        // definition, so it cannot be confidently attributed to THIS seed: it comes back a marked
        // FRONTIER carrying the SORTED candidate definition ids and is NOT ascended - the walk never
        // guesses, and nothing reachable only by ascending past it appears.
        let p = Projector::open(":memory:", "test").unwrap();

        // `target` is defined in TWO files, so a cross-file call to `target` is multi-candidate. Fold
        // e.rs BEFORE a.rs so the natural (rowid) order is [e, a]; only `ORDER BY id` in
        // `definitions_with_suffix` yields the asserted [a, b]-sorted candidate list.
        apply_batch_def(&p, 1, "src/e.rs", "target", 1, true);
        apply_batch_def(&p, 2, "src/a.rs", "target", 1, true); // the SEED
        apply_batch_def(&p, 3, "src/f.rs", "amb", 1, true); // the ambiguous caller
        apply_batch_def(&p, 4, "src/g.rs", "over", 1, true); // amb's own caller (must NOT be reached)
                                                             // amb calls `target` cross-file (multi-candidate); over calls amb (only reachable by ascending
                                                             // past the frontier).
        apply_batch_ref_caller(&p, 5, "src/f.rs", "target", "amb");
        apply_batch_ref_caller(&p, 6, "src/g.rs", "amb", "over");

        let cg = p
            .calls(
                &["src/a.rs::target".to_string()],
                Direction::Up,
                5,
                TIER_INFERRED,
            )
            .unwrap();
        let node = |id: &str| cg.nodes.iter().find(|n| n.node.id == id);
        let node_ids = |cg: &CallGraph| -> Vec<String> {
            let mut v: Vec<String> = cg.nodes.iter().map(|n| n.node.id.clone()).collect();
            v.sort();
            v
        };

        // The ambiguous caller is a marked FRONTIER at layer 1, carrying its SORTED candidate defs.
        // TEETH: e.rs was folded before a.rs, so a missing `ORDER BY id` would return [e, a]; only the
        // real sort yields [a, e].
        let amb =
            node("src/f.rs::amb").expect("the ambiguous caller is returned as a frontier node");
        assert_eq!(
            amb.layer, 1,
            "the frontier caller is one hop up from the seed"
        );
        assert_eq!(
            amb.frontier,
            Some(vec!["src/a.rs::target".to_string(), "src/e.rs::target".to_string()]),
            "the frontier carries its candidate definition ids SORTED by id - even though e.rs was folded first",
        );
        assert_eq!(
            cg.nodes.iter().filter(|n| n.frontier.is_some()).count(),
            1,
            "exactly one node is a frontier - the multi-candidate caller",
        );

        // NOT ASCENDED: nothing reachable ONLY by ascending past the frontier appears (`over`), and
        // the other same-named definition is never descended into (`e.rs::target`).
        for hidden in ["src/g.rs::over", "src/e.rs::target"] {
            assert!(
                node(hidden).is_none(),
                "{hidden} lies beyond the un-ascended frontier and must not be reached; nodes were {:?}",
                node_ids(&cg),
            );
        }
        assert_eq!(
            node_ids(&cg),
            vec!["src/a.rs::target".to_string(), "src/f.rs::amb".to_string()],
            "the reached set is the seed plus the marked frontier caller, nothing more",
        );
        // The edge to the frontier keeps the real CALLS direction, redirected onto the seed def.
        assert_eq!(
            cg.edges
                .iter()
                .map(|e| (e.edge.from.clone(), e.edge.to.clone()))
                .collect::<Vec<_>>(),
            vec![("src/f.rs::amb".to_string(), "src/a.rs::target".to_string())],
            "one caller edge, onto the seed def, marking the frontier",
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
    fn decision_fold_projects_no_agent_node_or_decided_edge() {
        // De-noise (spec 43): the acting persona is run machinery, not the target project, so a
        // DecisionMade - even carrying an event actor - projects NO KIND_AGENT node and NO REL_DECIDED
        // attribution edge. Its CONTENT survives: the decision node and its GOVERNS edge to the code.
        let p = Projector::open(":memory:", "test").unwrap();
        let payload = serde_json::json!({"id": "d1", "summary": "x", "governs": ["mod.rs"]});
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        e.meta.insert(META_ACTOR.to_string(), "agent-7".to_string());
        p.apply(&e).unwrap();
        let g = p.subgraph(&["d1".to_string()], 2).unwrap();
        // Content survives.
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "d1" && n.kind == KIND_DECISION),
            "the decision content node survives the de-noise"
        );
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_GOVERNS && x.from == "d1" && x.to == "mod.rs"),
            "the decision's GOVERNS edge to the code it concerns survives"
        );
        // Machinery is gone.
        assert!(
            !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected for the acting persona"
        );
        assert!(
            !g.edges.iter().any(|x| x.rel == REL_DECIDED),
            "no REL_DECIDED agent-attribution edge is projected"
        );
    }

    #[test]
    fn review_finding_creates_a_finding_node_about_each_file() {
        // A ReviewFinding folds into a KIND_FINDING node carrying its summary (and the reviewer
        // as a node ATTRIBUTE), and an ABOUT edge to each file it concerns. The finding is
        // reachable from the file it is ABOUT - the same traversal that returns the decisions
        // GOVERNING the file - so a later reviewer grounded on that file retrieves it through the
        // graph, not via hand-threaded prompts. De-noise (spec 43): the reviewer's provenance is
        // NOT projected as a KIND_AGENT node or a REL_RAISED edge (that agent attribution is run
        // machinery); the `by` reviewer remains only as the finding node's `by` attribute.
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
        // Machinery is gone: no reviewer agent node, no RAISED attribution edge.
        assert!(
            !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected for the reviewer"
        );
        assert!(
            !g.edges.iter().any(|x| x.rel == REL_RAISED),
            "no REL_RAISED agent-attribution edge is projected"
        );
    }

    #[test]
    fn review_finding_projects_no_raised_edge_even_with_an_event_actor() {
        // De-noise (spec 43): a ReviewFinding carrying an event actor still projects NO KIND_AGENT
        // node and NO REL_RAISED edge - the agent attribution is run machinery, dropped for both
        // the `by` reviewer and the actor override. Only the finding's CONTENT node survives.
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
            g.nodes
                .iter()
                .any(|n| n.id == "f1" && n.kind == KIND_FINDING),
            "the finding content node survives the de-noise"
        );
        assert!(
            !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected for the actor"
        );
        assert!(
            !g.edges.iter().any(|x| x.rel == REL_RAISED),
            "no REL_RAISED edge is projected even when an event actor is present"
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
        // no data.unit. (After the spec 43 de-noise a finding carries only its ABOUT edge - the
        // RAISED agent-attribution edge is no longer projected - so integration invalidates ABOUT.)
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
        // Spec 43 criterion 3 (LIFECYCLE survives the de-noise): disposition-expiry fired above
        // even though NO KIND_UNIT node was ever created for u1 - the invalidation reads the
        // finding's `$.unit` string attribute (a token), not a unit node.
        let (nodes, _) = all_nodes_edges(&p);
        assert!(
            !nodes.iter().any(|(_, k)| k == KIND_UNIT),
            "no KIND_UNIT node is projected, yet the integrate still drove disposition-expiry"
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
    fn unit_started_folds_to_no_node_or_edge() {
        // De-noise (spec 43): a unit, its assigned agent, and its dependency edges are all run
        // machinery, not the target project, so a UnitStarted folds to NOTHING in the graph - no
        // KIND_UNIT node, no KIND_AGENT node, no REL_ASSIGNED_TO / REL_BLOCKS edge. The event stays
        // in the log, where the run-tree (units/stages) and metrics read it. Seeding the graph on
        // any of the ids the event named returns an empty neighborhood.
        let p = Projector::open(":memory:", "test").unwrap();
        let payload =
            serde_json::json!({"unit": "u2", "criterion": "c", "agent": "impl", "needs": ["u1"]});
        let mut e = Event::new(TYPE_UNIT_STARTED, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        p.apply(&e).unwrap();
        for seed in [["u2"], ["u1"], ["impl"]] {
            let g = p
                .subgraph(&seed.iter().map(|s| s.to_string()).collect::<Vec<_>>(), 2)
                .unwrap();
            assert!(
                g.nodes.is_empty() && g.edges.is_empty(),
                "a UnitStarted event folds to no node/edge; seeding {seed:?} gave {g:?}"
            );
        }
    }

    #[test]
    fn machinery_is_gone_after_folding_a_full_run() {
        // Spec 43 criterion 1 (OWNS the machinery drop): after folding a run's FileTouched,
        // UnitStarted, UnitIntegrated, GateVerdict, DecisionMade, and ReviewFinding events, the
        // projection contains NO harness machinery anywhere - no KIND_AGENT / KIND_UNIT / KIND_GATE
        // node, no REL_TOUCHES edge, and no agent-attribution edge (RAISED or DECIDED, nor the
        // ASSIGNED_TO / BLOCKS / GATED_BY machinery edges). The graph models the TARGET PROJECT
        // (its code and the design memory about it), not the loop's own bookkeeping.
        let p = Projector::open(":memory:", "test").unwrap();

        // DecisionMade with an acting agent (event actor), governing a file.
        let mut d = Event::new(
            TYPE_DECISION_MADE,
            serde_json::to_vec(&serde_json::json!({
                "id": "d1", "summary": "x", "governs": ["combat.rs"], "supersedes": ""
            }))
            .unwrap(),
        );
        d.position = 1;
        d.meta
            .insert(META_ACTOR.to_string(), "rust-engineer".to_string());
        p.apply(&d).unwrap();

        // FileTouched (agent touches file).
        let mut ft = Event::new(
            TYPE_FILE_TOUCHED,
            serde_json::to_vec(&serde_json::json!({ "path": "combat.rs", "by": "rust-engineer" }))
                .unwrap(),
        );
        ft.position = 2;
        p.apply(&ft).unwrap();

        // GateVerdict on the file.
        let mut gv = Event::new(
            TYPE_GATE_VERDICT,
            serde_json::to_vec(
                &serde_json::json!({ "gate": "cargo test", "pass": true, "artifact": "combat.rs" }),
            )
            .unwrap(),
        );
        gv.position = 3;
        p.apply(&gv).unwrap();

        // UnitStarted (unit assigned to an agent, blocked by another unit).
        let mut us = Event::new(
            TYPE_UNIT_STARTED,
            serde_json::to_vec(&serde_json::json!({
                "unit": "u2", "criterion": "c", "agent": "impl", "needs": ["u1"]
            }))
            .unwrap(),
        );
        us.position = 4;
        p.apply(&us).unwrap();

        // ReviewFinding raised by a reviewer about the file.
        let mut rf = Event::new(
            TYPE_REVIEW_FINDING,
            serde_json::to_vec(&serde_json::json!({
                "id": "f1", "by": "tech-lens", "unit": "u2", "summary": "y", "about": ["combat.rs"]
            }))
            .unwrap(),
        );
        rf.position = 5;
        p.apply(&rf).unwrap();

        // UnitIntegrated.
        let mut ui = Event::new(
            TYPE_UNIT_INTEGRATED,
            serde_json::to_vec(&serde_json::json!({ "id": "u2", "commit": "abc" })).unwrap(),
        );
        ui.position = 6;
        p.apply(&ui).unwrap();

        let (nodes, edges) = all_nodes_edges(&p);
        for kind in [KIND_AGENT, KIND_UNIT, KIND_GATE] {
            assert!(
                !nodes.iter().any(|(_, k)| k == kind),
                "no {kind} machinery node is projected; got nodes {nodes:?}"
            );
        }
        for rel in [
            REL_TOUCHES,
            REL_RAISED,
            REL_DECIDED,
            REL_ASSIGNED_TO,
            REL_BLOCKS,
            REL_GATED_BY,
        ] {
            assert!(
                !edges.iter().any(|(_, r, _)| r == rel),
                "no {rel} machinery edge is projected; got edges {edges:?}"
            );
        }
    }

    #[test]
    fn content_survives_the_de_noise() {
        // Spec 43 criterion 2 (OWNS content preservation): the same fold that drops machinery STILL
        // produces the KIND_DECISION / KIND_FINDING / KIND_LESSON content nodes and their edges to
        // the code they concern (GOVERNS for a decision, ABOUT for a finding and a lesson). Only the
        // agent attribution is absent. It does NOT own the machinery drop (criterion 1).
        let p = Projector::open(":memory:", "test").unwrap();
        apply_decision(&p, 1, "d1", "use the shared authority", &["combat.rs"], "");
        apply_review_finding(&p, 2, "f1", "tech-lens", "u1/tech-lens#0", &["combat.rs"]);
        let mut le = Event::new(
            TYPE_LESSON_LEARNED,
            serde_json::to_vec(
                &serde_json::json!({ "id": "l1", "summary": "z", "about": ["combat.rs"] }),
            )
            .unwrap(),
        );
        le.position = 3;
        p.apply(&le).unwrap();

        let g = p.subgraph(&["combat.rs".to_string()], 2).unwrap();
        // Content nodes survive, reachable from the code they concern.
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "d1" && n.kind == KIND_DECISION),
            "the decision content node survives"
        );
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "f1" && n.kind == KIND_FINDING),
            "the finding content node survives"
        );
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "l1" && n.kind == KIND_LESSON),
            "the lesson content node survives"
        );
        // Content edges to the code survive.
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_GOVERNS && e.from == "d1" && e.to == "combat.rs"),
            "the decision's GOVERNS edge survives"
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_ABOUT && e.from == "f1" && e.to == "combat.rs"),
            "the finding's ABOUT edge survives"
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_ABOUT && e.from == "l1" && e.to == "combat.rs"),
            "the lesson's ABOUT edge survives"
        );
        // Only the agent attribution is absent.
        assert!(
            !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no agent attribution node survives"
        );
        assert!(
            !g.edges
                .iter()
                .any(|e| e.rel == REL_RAISED || e.rel == REL_DECIDED),
            "no agent attribution edge survives"
        );
    }

    #[test]
    fn consumers_read_the_log_not_the_dropped_machinery_nodes() {
        // Spec 43 criterion 4 (OWNS the safe-consumer guarantee): metrics folds the EVENT LOG, never
        // the graph, so dropping the unit/gate nodes cannot change what it reports. Fold a run's
        // UnitStarted + GateVerdicts into the de-noised projection AND compute metrics from the same
        // events: the graph carries NO KIND_UNIT / KIND_GATE node, yet metrics still counts the unit
        // from the log - proving the consumer never depended on the dropped nodes. It does NOT own
        // content preservation (criterion 2).
        let unit_started = {
            // Production shape: the conductor emits UnitStarted with an `id` and `agent` (metrics
            // keys on `id`), plus the `unit` the fold once read.
            let mut e = Event::new(
                TYPE_UNIT_STARTED,
                serde_json::to_vec(&serde_json::json!({
                    "id": "u1", "unit": "u1", "criterion": "c", "agent": "impl", "needs": []
                }))
                .unwrap(),
            );
            e.position = 1;
            e
        };
        let gate_pass = {
            let mut e = Event::new(
                TYPE_GATE_VERDICT,
                serde_json::to_vec(&serde_json::json!({ "gate": "cargo test", "pass": true }))
                    .unwrap(),
            );
            e.position = 2;
            e
        };
        let events = vec![unit_started, gate_pass];

        // Fold into the (de-noised) graph.
        let p = Projector::open(":memory:", "test").unwrap();
        for e in &events {
            p.apply(e).unwrap();
        }
        let (nodes, _) = all_nodes_edges(&p);
        assert!(
            !nodes.iter().any(|(_, k)| k == KIND_UNIT || k == KIND_GATE),
            "the de-noised graph carries no unit/gate machinery node; got {nodes:?}"
        );

        // Metrics reads the LOG and is unaffected by the absent nodes.
        let m = crate::metrics::project(&events);
        assert_eq!(
            m.units_started, 1,
            "metrics counts the unit from the event log, not the (absent) graph node"
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
        let removed = alpha.prune(&["drop-d".to_string()], None).unwrap();
        assert_eq!(
            removed.nodes, 1,
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
        // project-UNIQUE neighbor decision ("only-alpha" / "only-beta") governing the same file.
        // Each decision carries an actor, but the de-noise (spec 43) drops it: no agent node is
        // projected, so scoping is proven over the surviving decision/file nodes. On a shared
        // backend the two projects occupy
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
        assert!(
            ag.nodes.iter().any(|n| n.id == "only-alpha"),
            "alpha's own node only-alpha stays reachable under scope, got {ag:?}"
        );
        // De-noise (spec 43): the DECIDING agent is never a node, in either project's read.
        assert!(
            !ag.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected (the actor is dropped), got {ag:?}"
        );

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
        assert!(
            bg.nodes.iter().any(|n| n.id == "only-beta"),
            "beta's own node only-beta stays reachable under scope, got {bg:?}"
        );
        assert!(
            !bg.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected in beta's read either, got {bg:?}"
        );
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
        // alpha's own nodes go missing (under-derivation). Each decision carries an actor, but the
        // de-noise (spec 43) drops it - no agent node is re-derived - so the scoped set is the
        // shared file, the shared decision id, and the project-unique decision, exercising node,
        // edge, and traversal re-derivation together.
        let want = |own_decision: &str| -> BTreeSet<String> {
            ["shared.rs", "d1", own_decision]
                .iter()
                .map(|s| s.to_string())
                .collect()
        };
        assert_eq!(
            scoped.get("alpha"),
            Some(&want("only-alpha")),
            "the rebuilt alpha subgraph reaches EXACTLY alpha's own nodes (no beta leak, none \
             missing), got {scoped:?}"
        );
        assert_eq!(
            scoped.get("beta"),
            Some(&want("only-beta")),
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

    /// Every user-defined index name on the graph tables, read from `sqlite_master` (the
    /// implicit `sqlite_autoindex_*` primary-key indexes are excluded), so a test can assert an
    /// additive migration created a named index.
    fn index_names(p: &Projector) -> BTreeSet<String> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master
                  WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// The `EXPLAIN QUERY PLAN` `detail` lines for `sql`, so a test can assert the planner chose a
    /// named index rather than a full table scan.
    fn query_plan(p: &Projector, sql: &str) -> Vec<String> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        stmt.query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn additive_indexes_exist_and_the_pinned_name_suffix_query_uses_its_index() {
        // Spec 45 criterion 4: the two additive indexes that keep whole-graph and directed reads
        // sub-linear are present after migration and are actually USED. This test names NO
        // feature-gated symbol, so it compiles and runs identically on the default and
        // `--no-default-features` lanes - the "in BOTH feature lanes" clause is satisfied by
        // construction. `open` runs the migration, so a fresh db already carries both indexes.
        let p = Projector::open(":memory:", "test").unwrap();
        let names = index_names(&p);
        assert!(
            names.contains("idx_edges_live_rel_from"),
            "the partial live-edge index edges(rel, from_id) WHERE valid_to IS NULL is present; got {names:?}"
        );
        assert!(
            names.contains("idx_nodes_name_suffix"),
            "the entity-name-suffix expression index on nodes is present; got {names:?}"
        );

        // A few code-entity nodes (`<file>::<name>` ids) so the resolution query has a realistic
        // target space to resolve `Foo` against.
        {
            let conn = p.conn.lock().unwrap();
            for id in ["a.rs::Foo", "b.rs::Foo", "c.rs::Bar"] {
                conn.execute(
                    "INSERT INTO nodes (id, kind, attrs, project) VALUES (?1, ?2, NULL, 'test')",
                    params![id, KIND_CODE_ENTITY],
                )
                .unwrap();
            }
        }

        // ARE USED (expression index): a name-resolution query phrased with the PINNED expression
        // `substr(id, instr(id, '::') + 2)` - identical to the fold's twin on `to_id` - hits the
        // expression index, not a full scan. SQLite uses an expression index only when the query's
        // expression matches it, so this pins the exact phrasing the coming cross-file resolution
        // (spec 46) MUST reuse or silently miss the index.
        let plan = query_plan(
            &p,
            "SELECT id FROM nodes WHERE substr(id, instr(id, '::') + 2) = 'Foo'",
        );
        assert!(
            plan.iter().any(|d| d.contains("idx_nodes_name_suffix")),
            "the pinned substr(id, instr(id,'::')+2) resolution uses idx_nodes_name_suffix; plan was {plan:?}"
        );

        // ARE USED (partial index): the relationship-scoped forward scan
        // (`rel = ? AND valid_to IS NULL`, the shape a directed CALLS traversal walks) is served by
        // the partial live-edge index. The query carries the exact `valid_to IS NULL` term, so
        // SQLite is allowed to pick the partial index over the plain `from_id`/`to_id` indexes.
        let plan = query_plan(
            &p,
            "SELECT to_id FROM edges WHERE rel = 'CALLS' AND valid_to IS NULL",
        );
        assert!(
            plan.iter().any(|d| d.contains("idx_edges_live_rel_from")),
            "the relationship-scoped forward scan uses the partial idx_edges_live_rel_from; plan was {plan:?}"
        );
    }
}
