//! Periphery (integration) test for spec 45 criterion 4: the additive read-path indexes
//! `Projector::open` creates survive the persist -> reopen boundary of a REAL on-disk graph.db
//! and remain usable by a completely INDEPENDENT reader connection, and (spec 119 criterion 2) a
//! reopen retires the node name-suffix index an older open created. This runs OUTSIDE the crate,
//! over the public `Projector::open` surface plus a raw `rusqlite` connection to the built file -
//! the durable read-path foundation the coming graph inspector actually opens.
//!
//! The implementer's inside-out unit test proves the indexes exist and are chosen by the planner
//! on a FRESH `:memory:` connection. A `:memory:` db is torn down on close, so that test is
//! structurally blind to the two boundaries this layer guards:
//!
//!   1. IDEMPOTENT PERSISTENCE. The migration relies on `CREATE INDEX IF NOT EXISTS` being a no-op
//!      on re-open; a regression to a bare `CREATE INDEX` would make the SECOND `Projector::open`
//!      of an already-built file return `Err` (public-API-observable) and could leave a duplicate
//!      index. This asserts a re-open succeeds and each named index persisted EXACTLY ONCE.
//!   2. DURABLE READ PATH. An index is written by the projector but consumed by a DIFFERENT
//!      reader; this opens the built file with an independent `rusqlite::Connection` and asserts
//!      that reader finds the indexes and that the planner picks each for the exact pinned query
//!      shape the read path must reuse - the sub-linear guarantee has to hold for the foreign
//!      reader, not just the connection that wrote it.
//!
//! Mirrors the established `the_confidence_tier_persists_across_a_reopen_of_an_on_disk_graph`
//! on-disk-reopen precedent. It names no feature-gated symbol, so it compiles and runs identically
//! on the default and `--no-default-features` lanes.

mod common;

use rigger::contextgraph::sqlite::Projector;

/// The number of user-defined indexes named `name` on the db at `conn` (the implicit
/// `sqlite_autoindex_*` primary-key indexes never match), read from `sqlite_master` - so a test can
/// assert an additive migration left EXACTLY ONE across a reopen.
fn index_count(conn: &rusqlite::Connection, name: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [name],
        |r| r.get(0),
    )
    .unwrap()
}

/// The pinned leaf query over the project bound as `?1`: the store form of a node's leaf, its
/// non-empty `name`, equal to `Foo`. SQLite serves the leaf index only to this exact expression.
const LEAF_QUERY: &str =
    "SELECT id FROM nodes WHERE project = ?1 AND NULLIF(json_extract(attrs, '$.name'), '') = 'Foo'";

/// The ids [`LEAF_QUERY`] answers over `project`, sorted.
fn leaf_ids(conn: &rusqlite::Connection, project: &str) -> Vec<String> {
    let mut stmt = conn.prepare(LEAF_QUERY).unwrap();
    let mut out: Vec<String> = stmt
        .query_map([project], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    out.sort();
    out
}

/// The `EXPLAIN QUERY PLAN` `detail` lines for `sql`, so a test can assert the planner chose a named
/// index over a full table scan on the persisted db read through a foreign connection.
fn query_plan(
    conn: &rusqlite::Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    stmt.query_map(params, |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn additive_indexes_persist_and_a_foreign_reader_uses_them_across_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.db");
    let path = path.to_str().unwrap();

    // Build: a fresh projector creates the schema and runs the additive-index migration, then the
    // connection is closed - the indexes now live only on disk.
    {
        Projector::open(path, "test").unwrap();
    }

    // An older open left the node name-suffix index the edge side no longer reads.
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch("CREATE INDEX IF NOT EXISTS idx_nodes_name_suffix ON nodes(substr(id, instr(id, '::') + 2))")
        .unwrap();

    // Re-open the SAME on-disk file. This re-runs the additive migration; `CREATE INDEX IF NOT
    // EXISTS` must make it an idempotent no-op - a `.unwrap()` that panicked here would catch a
    // regression to a bare `CREATE INDEX` (which errors when the index already exists) - and
    // retires the name-suffix index.
    {
        Projector::open(path, "test").unwrap();
    }

    // An independent reader opens the built db cold - the read-path shape a separate consumer uses.
    let reader = rusqlite::Connection::open(path).unwrap();

    // A few named code-entity nodes, one nested, so the leaf index has a realistic target space
    // to resolve a leaf against.
    for (id, name) in [
        ("a.rs::Foo", "Foo"),
        ("b.rs::T::Foo", "Foo"),
        ("c.rs::Bar", "Bar"),
    ] {
        common::cli::insert_node(
            &reader,
            "test",
            id,
            "code-entity",
            Some(&serde_json::json!({ "name": name }).to_string()),
        );
    }

    // 1. IDEMPOTENT PERSISTENCE: each additive index survived on disk across the two opens, exactly
    // once - the re-open neither errored nor duplicated it.
    assert_eq!(
        index_count(&reader, "idx_edges_live_rel_from"),
        1,
        "the partial live-edge index persisted exactly once across the reopen"
    );
    assert_eq!(
        index_count(&reader, "idx_nodes_leaf"),
        1,
        "the leaf expression index persisted exactly once across the reopen"
    );
    assert_eq!(
        index_count(&reader, "idx_nodes_name_suffix"),
        0,
        "the reopen retired the node name-suffix index an older open created"
    );

    // 2a. DURABLE READ PATH (expression index): the foreign reader's leaf query, phrased with the
    // PINNED leaf expression, both uses the index and resolves the leaf to every definition named
    // it, nested or not. SQLite uses an expression index only when the query's expression matches
    // it, so this pins the exact phrasing every node-side reader MUST reuse or silently miss it.
    let plan = query_plan(&reader, LEAF_QUERY, ["test"]);
    assert!(
        plan.iter().any(|d| d.contains("idx_nodes_leaf")),
        "the foreign reader's pinned leaf query uses idx_nodes_leaf; plan was {plan:?}"
    );
    assert_eq!(
        leaf_ids(&reader, "test"),
        vec!["a.rs::Foo".to_string(), "b.rs::T::Foo".to_string()],
        "the index-served resolution returns both `Foo` definitions and not the `Bar` node"
    );

    // 2b. DURABLE READ PATH (partial index): the relationship-scoped forward scan carrying the exact
    // `valid_to IS NULL` term - the shape a directed CALLS traversal walks - is served by the partial
    // live-edge index for the foreign reader. The query must carry that exact term for SQLite to be
    // allowed to pick the partial index over the plain `from_id` index (the planner decision is
    // schema-driven, so an empty edges table exercises it exactly as a populated one).
    let plan = query_plan(
        &reader,
        "SELECT to_id FROM edges WHERE rel = 'CALLS' AND valid_to IS NULL",
        [],
    );
    assert!(
        plan.iter().any(|d| d.contains("idx_edges_live_rel_from")),
        "the foreign reader's relationship-scoped forward scan uses the partial idx_edges_live_rel_from; plan was {plan:?}"
    );

    // 2c. DURABLE READ PATH (edge text index, spec 119 criterion 2): a live edge read binding a
    // leaf, its target text compared with the leaf and the sigil-prefixed leaf, is served by the
    // partial edge text index for the foreign reader, the relation written `+rel` so the planner
    // seeks by the bound leaf rather than by every live edge of the relation.
    let plan = query_plan(
        &reader,
        "SELECT from_id FROM edges WHERE +rel = 'CALLS' AND valid_to IS NULL
            AND substr(to_id, instr(to_id, '::') + 2) IN (?1, '%' || ?1)",
        ["new"],
    );
    assert!(
        plan.iter()
            .any(|d| d.contains("SEARCH edges USING INDEX idx_edges_live_to_name_suffix")),
        "the foreign reader's leaf-bound edge read seeks idx_edges_live_to_name_suffix; plan was {plan:?}"
    );
}

/// The load-bearing ORDERING guard. `migrate_indexes` documents that it MUST run AFTER
/// `migrate_project_scope`: on a PRE-spec-28 on-disk graph.db - the old `nodes(id PRIMARY KEY, ...)`
/// shape with no `project` column - the scope migration DROPs and recreates `nodes` to add the
/// composite `(id, project)` key, and an `idx_nodes_leaf` created any earlier is silently
/// stranded when the old table is dropped. That degrades leaf resolution to a full scan
/// on every legacy upgrade with EVERY test still green - the exact silent-degrade class this pin
/// exists to catch.
///
/// The fresh-db test above is structurally blind to it: a fresh `SCHEMA` already carries
/// `nodes.project`, so `migrate_project_scope` skips its DROP branch entirely and the index is never
/// at risk of the rename/drop. Only a PRE-spec-28 upgrade fixture exercises the DROP branch, so only
/// this test guards the ordering. Reordering the three `migrate_*` calls in `Projector::open` reddens
/// HERE (the nodes index goes ABSENT), loudly, where nothing else would.
///
/// Mirrors the in-crate `a_pre_project_graph_db_migrates_additively_backfilling_the_openers_identity`
/// precedent for hand-building the pre-spec-28 fixture, run over the public `Projector::open` surface
/// plus a foreign `rusqlite` reader. It names no feature-gated symbol, so it runs identically on the
/// default and `--no-default-features` lanes.
#[test]
fn the_leaf_index_survives_the_pre_spec_28_scope_migrations_nodes_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.db");
    let path = path.to_str().unwrap();

    // Hand-build the PRE-spec-28 schema exactly as pre-scope code wrote it: `nodes` keyed by `id`
    // ALONE with NO `project` column (plus the old `edges`/`aliases`/`applied` tables), seeded with a
    // few named code-entity nodes. Because `nodes` lacks `project`, opening it will drive
    // `migrate_project_scope` into its DROP-and-recreate-`nodes` branch - the branch the fresh-db
    // path never takes.
    {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE nodes (id TEXT PRIMARY KEY, kind TEXT NOT NULL, attrs TEXT);
             CREATE TABLE edges (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
               valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL
             );
             CREATE TABLE aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
             CREATE TABLE applied (position INTEGER PRIMARY KEY);
             INSERT INTO nodes (id, kind, attrs) VALUES ('a.rs::Foo', 'code-entity', json_object('name', 'Foo'));
             INSERT INTO nodes (id, kind, attrs) VALUES ('b.rs::T::Foo', 'code-entity', json_object('name', 'Foo'));
             INSERT INTO nodes (id, kind, attrs) VALUES ('c.rs::Bar', 'code-entity', json_object('name', 'Bar'));",
        )
        .unwrap();
    }

    // Open through the scoped constructor. The migration chain runs: `migrate_project_scope` DROPs
    // and recreates `nodes` (the pre-28 branch), THEN `migrate_indexes` builds the indexes on the
    // FINAL table shape. If a future edit runs `migrate_indexes` before the scope migration, the
    // nodes index it creates is dropped with the old table and this open leaves it ABSENT.
    Projector::open(path, "legacy-proj").unwrap();

    // A foreign reader inspects the built db cold - the read-path shape a separate consumer opens.
    let reader = rusqlite::Connection::open(path).unwrap();

    // THE GUARD: the leaf index SURVIVED the pre-28 rebuild of `nodes`, present exactly once.
    // This count is 0 the instant `migrate_indexes` runs before `migrate_project_scope`.
    assert_eq!(
        index_count(&reader, "idx_nodes_leaf"),
        1,
        "idx_nodes_leaf must survive migrate_project_scope's DROP/recreate of nodes on a \
         pre-spec-28 upgrade - reordering the migrate_* calls in open() would strand it"
    );
    // The partial edge index also lands on the pre-28 path. `edges` is migrated by `ALTER ... ADD
    // COLUMN` (which preserves indexes either way), so it is not at risk of the rename/drop, but it
    // must exist after the legacy open all the same.
    assert_eq!(
        index_count(&reader, "idx_edges_live_rel_from"),
        1,
        "the partial live-edge index also exists after the pre-spec-28 upgrade open"
    );

    // Not merely present - BOUND TO THE RECREATED TABLE and usable: the foreign reader's pinned
    // leaf query both picks the index and resolves the leaf to every migrated definition node,
    // proving the index attached to the new `nodes`, not a ghost of the dropped one.
    let plan = query_plan(&reader, LEAF_QUERY, ["legacy-proj"]);
    assert!(
        plan.iter().any(|d| d.contains("idx_nodes_leaf")),
        "the migrated-table resolution uses idx_nodes_leaf; plan was {plan:?}"
    );
    assert_eq!(
        leaf_ids(&reader, "legacy-proj"),
        vec!["a.rs::Foo".to_string(), "b.rs::T::Foo".to_string()],
        "the index-served resolution over the migrated nodes returns both `Foo` definitions"
    );
}
