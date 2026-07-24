//! Periphery (integration) test for spec 45 criterion 4: the two additive read-path indexes
//! `Projector::open` creates survive the persist -> reopen boundary of a REAL on-disk graph.db
//! and remain usable by a completely INDEPENDENT reader connection. This runs OUTSIDE the crate,
//! over the public `Projector::open` surface plus a raw `rusqlite` connection to the built file -
//! the durable read-path foundation the coming graph inspector actually opens.
//!
//! The implementer's inside-out unit test proves both indexes exist and are chosen by the planner
//! on a FRESH `:memory:` connection. A `:memory:` db is torn down on close, so that test is
//! structurally blind to the two boundaries this layer guards:
//!
//!   1. IDEMPOTENT PERSISTENCE. The migration relies on `CREATE INDEX IF NOT EXISTS` being a no-op
//!      on re-open; a regression to a bare `CREATE INDEX` would make the SECOND `Projector::open`
//!      of an already-built file return `Err` (public-API-observable) and could leave a duplicate
//!      index. This asserts a re-open succeeds and each named index persisted EXACTLY ONCE.
//!   2. DURABLE READ PATH. An index is written by the projector but consumed by a DIFFERENT
//!      reader; this opens the built file with an independent `rusqlite::Connection` and asserts
//!      that reader finds both indexes and that the planner picks each for the exact pinned query
//!      shape the read path must reuse - the sub-linear guarantee has to hold for the foreign
//!      reader, not just the connection that wrote it.
//!
//! Mirrors the established `the_confidence_tier_persists_across_a_reopen_of_an_on_disk_graph`
//! on-disk-reopen precedent. It names no feature-gated symbol, so it compiles and runs identically
//! on the default and `--no-default-features` lanes.

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

/// The `EXPLAIN QUERY PLAN` `detail` lines for `sql`, so a test can assert the planner chose a named
/// index over a full table scan on the persisted db read through a foreign connection.
fn query_plan(conn: &rusqlite::Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    stmt.query_map([], |r| r.get::<_, String>(3))
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

    // Re-open the SAME on-disk file. This re-runs the additive migration; `CREATE INDEX IF NOT
    // EXISTS` must make it an idempotent no-op - a `.unwrap()` that panicked here would catch a
    // regression to a bare `CREATE INDEX` (which errors when the index already exists).
    {
        Projector::open(path, "test").unwrap();
    }

    // An independent reader opens the built db cold - the read-path shape a separate consumer uses.
    let reader = rusqlite::Connection::open(path).unwrap();

    // A few code-entity nodes (`<file>::<name>` ids) so the expression index has a realistic target
    // space to resolve a bare name against.
    for id in ["a.rs::Foo", "b.rs::Foo", "c.rs::Bar"] {
        reader
            .execute(
                "INSERT INTO nodes (id, kind, attrs, project) VALUES (?1, 'code-entity', NULL, 'test')",
                [id],
            )
            .unwrap();
    }

    // 1. IDEMPOTENT PERSISTENCE: each additive index survived on disk across the two opens, exactly
    // once - the re-open neither errored nor duplicated it.
    assert_eq!(
        index_count(&reader, "idx_edges_live_rel_from"),
        1,
        "the partial live-edge index persisted exactly once across the reopen"
    );
    assert_eq!(
        index_count(&reader, "idx_nodes_name_suffix"),
        1,
        "the entity-name-suffix expression index persisted exactly once across the reopen"
    );

    // 2a. DURABLE READ PATH (expression index): the foreign reader's name-resolution query, phrased
    // with the PINNED expression `substr(id, instr(id, '::') + 2)`, both uses the index and resolves
    // the bare name to every matching definition node. SQLite uses an expression index only when the
    // query's expression matches it, so this pins the exact phrasing the coming cross-file resolution
    // MUST reuse or silently miss the index.
    let resolve = "SELECT id FROM nodes WHERE substr(id, instr(id, '::') + 2) = 'Foo'";
    let plan = query_plan(&reader, resolve);
    assert!(
        plan.iter().any(|d| d.contains("idx_nodes_name_suffix")),
        "the foreign reader's pinned substr(id,instr(id,'::')+2) resolution uses idx_nodes_name_suffix; plan was {plan:?}"
    );
    let mut resolved: Vec<String> = {
        let mut stmt = reader.prepare(resolve).unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    resolved.sort();
    assert_eq!(
        resolved,
        vec!["a.rs::Foo".to_string(), "b.rs::Foo".to_string()],
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
    );
    assert!(
        plan.iter().any(|d| d.contains("idx_edges_live_rel_from")),
        "the foreign reader's relationship-scoped forward scan uses the partial idx_edges_live_rel_from; plan was {plan:?}"
    );
}
