//! The passed-over fixtures (spec 124, PASSED OVER WITH ITS CAUSE): the rows a graph file's
//! `passed_over` table holds and the cause a fold pass records for a payload the judge rejects,
//! shared by the root suites and the graph crate's inline tests.

/// The cause a fold pass records, under the class `class` (`payload rejected` for an event's own
/// payload, `fold failed` for an event of a re-extracted batch), for the payload `data` of a
/// `type_` event the judge rejects ([`rigger::contextgraph::check_fold_payload`]): the class, then
/// the judge's text.
pub fn judged_cause(class: &str, type_: &str, data: &[u8]) -> String {
    let why = rigger::contextgraph::check_fold_payload(type_, data).unwrap_err();
    format!("{class}: {why}")
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
