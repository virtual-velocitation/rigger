//! The passed-over fixtures (spec 124, PASSED OVER WITH ITS CAUSE): the rows a graph file's
//! `passed_over` table holds, the cause a fold pass records for a payload the judge rejects, and
//! how the lines reporting what the file passed over count it (CATCH-UP LINES), shared by the
//! root suites, the binary's inline tests and the graph crate's inline tests.

/// How a line reporting what a graph file passed over counts `rejected` rejected payloads (spec
/// 124, CATCH-UP LINES): their total, and their count by cause in parentheses - as `rigger setup`'s
/// passed-over line and `rigger validate`'s note ([`passed_over_note`]) both spell them, each
/// ending with [`RETRIED_BY_A_LATER_RULE`].
pub fn passed_over_counts(rejected: usize) -> (usize, String) {
    (rejected, format!("({rejected} rejected payload(s))"))
}

/// What every line reporting what a graph file passed over ends by saying of those events.
pub const RETRIED_BY_A_LATER_RULE: &str = "a rebuild under a later fold rule retries them";

/// The note `rigger validate` prints while `graph.db` counts events passed over, by cause (spec
/// 124, CATCH-UP LINES): `rejected` rejected payloads ([`passed_over_counts`]).
pub fn passed_over_note(rejected: usize) -> String {
    let (n, by_cause) = passed_over_counts(rejected);
    format!("note: graph.db passed over {n} event(s) {by_cause}; {RETRIED_BY_A_LATER_RULE}")
}

/// The `passed_over` row a fold pass records for the event `e`, whose own payload the judge
/// rejects ([`rigger::contextgraph::check_fold_payload`]): its position and its cause, the
/// `payload rejected` class, then the judge's text.
pub fn rejected_row(e: &rigger::eventstore::Event) -> (u64, String) {
    let why = rigger::contextgraph::check_fold_payload(&e.type_, &e.data).unwrap_err();
    (
        e.position,
        format!("{}: {why}", rigger::contextgraph::sqlite::PAYLOAD_REJECTED),
    )
}

/// Every row of the `passed_over` table of the graph file at `graph_db` (spec 124, PASSED OVER WITH
/// ITS CAUSE), ascending: each position a fold recorded passed over, with its cause.
pub fn passed_over_rows(graph_db: &str) -> Vec<(u64, String)> {
    let conn = rusqlite::Connection::open(graph_db).unwrap();
    let mut rows = conn
        .prepare("SELECT position, cause FROM passed_over ORDER BY position")
        .unwrap();
    let rows = rows.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    rows.map(Result::unwrap).collect()
}
