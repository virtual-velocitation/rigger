//! PERIPHERY (contract / API / integration) tests for spec 60, criterion 5 - the boundary
//! surface `rigger reset --derived` opened, tested from OUTSIDE the code that implements it.
//!
//! The criterion's own behaviour - which rows the prune keeps, what the file weighs afterwards,
//! what the command printed, the loud refusal on a backend that cannot compact - is pinned by
//! `tests/reset_derived_compaction.rs`. This file does not re-litigate any of it. It covers the
//! boundaries that unit is judged at but which its own tests cannot reach from inside a
//! single-project, single-store, single-invocation view:
//!
//!   1. **The namespace boundary of a store-level prune.** `Store::prune_derived_index` works on
//!      the backend DIRECTLY, beneath the `Namespaced` decorator that scopes every ordinary read
//!      and write, and one backend file is documented to hold many projects. So the prune's reach
//!      is a contract in its own right: it must delete inside the namespace it was handed and
//!      nowhere else, and it must match that namespace LITERALLY - a project identity carrying a
//!      SQL wildcard (`my_repo`) must not sweep in the neighbour a `LIKE` match would (`myXrepo`).
//!   2. **The API edges of the new store primitive.** What it reports when it is asked for
//!      nothing, asked about a metadata key no row carries, or pointed at a namespace that does
//!      not exist - and that the per-type report keeps the CALLER's order, zeros included.
//!   3. **The cross-module seam between the command and `ingest`.** The command prunes the four
//!      derived index types because `rigger::ingest` declares them; nothing else may decide that
//!      list. Binding the command's printed report to the constant is what makes a FIFTH derived
//!      type added to `ingest` provably covered by the prune instead of silently unpruned.
//!   4. **The event-store contract on a compacted stream.** The prune leaves holes in a stream's
//!      revisions, which is why the sqlite `append` now reads the cursor as `MAX(revision)`. That
//!      changes an `EventStore` promise every caller depends on - `ExpectedRevision` - so the
//!      optimistic-concurrency check is exercised at its new edges, not just `Any`.
//!   5. **The namespace boundary the whole store-MAINTENANCE family shares.** The prune is the
//!      third prefix-taking maintenance op on the concrete store, beside `has_stream_prefix` and
//!      `rename_stream_prefix`. `Namespaced::prefix_for` is published so all of them address the
//!      same boundary; the property that claim is worth is that a project whose streams were
//!      MOVED by the identity migration is still found and compacted at its new identity.
//!   6. **The shipped operator-facing artifacts.** The two rendered consumer documents and the
//!      binary's own usage registry are what an operator actually reads. They are asserted on the
//!      COMMITTED bytes and on the RUNNING binary, with no render in the loop.
//!   7. **The two-store dissociation the composition made load-bearing.** `reset` now drives TWO
//!      prunes over TWO different stores, and each states in its own output that it leaves the
//!      other alone - `--runs` prints that it "deletes no event" from the log, the shipped
//!      `--derived` guidance says the live graph is unchanged. Neither claim is reachable from a test that runs
//!      one mode against one store, so both are pinned here against a project whose event log AND
//!      context graph are populated, together with the composition that follows from them: running
//!      the two together lands EXACTLY the two effects, neither more nor less.
//!   8. **The run history, read back through the binary after a compaction.** The shipped guidance
//!      promises that the whole run history `rigger stats` reads survives the prune. Rows surviving
//!      in the table is not that promise: the run read-model rides the namespace-scoped GLOBAL read
//!      (a `LIKE`-filtered scan across the file), a different path from the `read_stream` the
//!      revision-cursor test drives, and it is the one an operator actually looks at.
//!   9. **The STREAM boundary inside one namespace.** The prune ranks a key's recordings within
//!      each STREAM, but its candidate set is chosen by type and by namespace prefix - never by
//!      stream - and a project namespace holds more than one stream. So "latest per key" is a
//!      per-stream fact that no single-stream fixture can distinguish from a per-namespace one,
//!      and neither can the per-stream revision cursor the compaction gapped.
//!  10. **The OTHER command the criterion names.** Criterion 5 requires that `rigger status` AND
//!      `rigger validate` read the compacted store correctly. `validate` is pinned by the unit's
//!      own suite; `status` walks a different path from either that suite or item 8 above - the
//!      per-stream read, the current-run slice, and the replay driver's frontier.
//!  11. **The rows the prune shares with the STORAGE GUARD.** The compaction is not the only
//!      thing in this store that reads a replay key: the spec-60 storage guard decides whether an
//!      append is redundant by asking which generation a subject is CURRENTLY at, and it answers
//!      that from the very rows the prune deletes (the latest recorded position of each covered
//!      key) inside the very file the prune then `VACUUM`s. So "keep the latest recording of every
//!      key" is not only a statement about what survives - it is the precondition of a defense
//!      that suppresses. A prune that kept the WRONG recording of a key would leave a log an
//!      operator cannot tell apart from a healthy one and a guard that has quietly changed its
//!      mind about which content is current. Neither layer's own tests can see this: the guard's
//!      suite never prunes and the compaction's suite never configures a guard.
//!  12. **The compare-and-append that rides ABOVE the gaps.** The derived index shares the run
//!      stream with the run's own events, so this compaction is the first and only thing in the
//!      project that deletes rows from a stream an operator keeps writing to - and the prune
//!      accounts for the holes it leaves against exactly ONE consumer, the sqlite `append`, whose
//!      cursor is now `MAX(revision)`. One caller lives ABOVE that boundary and supplies an
//!      `ExpectedRevision::Exact` of its own: the compare-and-append behind `rigger result
//!      --if-absent`, the write that moves a run past a spawn whose agent died without
//!      self-reporting. It must take its expectation from the head event's own revision and never
//!      from how many events the read returned, because a conflict it cannot satisfy is not a
//!      failed write - the loop re-reads and retries it forever. Invisible to both sides: the
//!      compaction suites record no result, and every test of that write runs on a densely
//!      numbered stream, where the two cursors agree.
//!  13. **The OTHER reader that same sentence names.** The shipped paragraph promises the run
//!      history "`rigger stats` and replay read" survives the prune. Item 8 above proves the
//!      `stats` half; `replay` is the other name in the sentence and it reaches the log by a
//!      THIRD path again - it lifts its BASELINE through the per-stream `read_stream`, which
//!      orders by the very REVISION sequence the compaction gapped, and folds that slice into a
//!      baseline whose unit outcomes are attributed POSITIONALLY, by the `RunStarted` that
//!      precedes them. Neither the global `LIKE`-filtered scan item 8 drives nor the current-run
//!      slice item 10 drives is that path, so an assumption of a dense revision sequence anywhere
//!      along it would print a different baseline column after the prune while every surviving
//!      row still looked perfectly intact in the table.
//!
//! Plus the command's own flag registry at the edges the composition opened: each mode named at
//! most once, and the two modes composing in EITHER order.

mod common;

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::Projection;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::{PrunedDerived, Store};
use rigger::eventstore::{
    ContentIdentity, Direction, Error, Event, EventStore, ExpectedRevision, META_GUARD_DEGRADED,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

/// One row of the event log as the table holds it: position, stream, type, id, payload bytes,
/// metadata, and per-stream revision. Comparing these tuples is what "untouched" MEANS - a row
/// that kept its bytes but was renumbered, or moved to another position, is not untouched.
type Row = (i64, String, String, String, Vec<u8>, String, i64);

fn raw_rows(db: &Path) -> Vec<Row> {
    let conn = rusqlite::Connection::open(db).expect("open the event log");
    let mut stmt = conn
        .prepare(
            "SELECT position, stream, type, id, data, meta, revision FROM events ORDER BY position",
        )
        .unwrap();
    let out = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    out
}

fn rows_in(rows: &[Row], prefix: &str) -> Vec<Row> {
    rows.iter()
        .filter(|r| r.1.starts_with(prefix))
        .cloned()
        .collect()
}

/// The replay key a row carries, if any, read out of its metadata exactly as the store reads it.
fn replay_key(row: &Row) -> Option<String> {
    let meta: serde_json::Value = serde_json::from_str(&row.5).ok()?;
    meta.get(rigger::ingest::META_REPLAY_KEY)?
        .as_str()
        .map(str::to_string)
}

/// The SAME two replay keys are recorded in EVERY seeded namespace below. A prune that partitioned
/// by content key alone - forgetting that the key is only meaningful WITHIN a stream - would sweep
/// every project's recordings of these keys together, which is precisely the failure the
/// namespace assertions exist to catch.
const KEY_DEF: &str = "gc/src/a.rs@h1#0";
const KEY_REF: &str = "gc/src/a.rs@h1#1";

fn keyed(type_: &str, data: Vec<u8>, key: &str, secs: u64) -> Event {
    Event::new(type_, data)
        .with_meta(rigger::ingest::META_REPLAY_KEY, key)
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
}

fn entity(name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "src/a.rs", "name": name, "kind": "function", "line": 1, "lang": "rust",
        "fresh": true,
    }))
    .unwrap()
}

fn edge(name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "file": "src/a.rs", "name": name, "lang": "rust" }))
        .unwrap()
}

/// Seed `project`'s namespace inside `backend` with `rounds` re-recordings of two derived keys -
/// the duplication the prune exists to shed - preceded by one non-derived event that must survive
/// any prune. Written THROUGH `Namespaced::new`, so the streams it creates are named by the very
/// code path the published `Namespaced::prefix_for` claims to speak for.
fn seed_namespace(backend: &Store, project: &str, rounds: u64) {
    let store = Namespaced::new(backend, project);
    let mut events = vec![Event::new(
        "RunStarted",
        format!(r#"{{"run":"{project}","criteria":["c"]}}"#).into_bytes(),
    )
    .with_valid_from(UNIX_EPOCH + Duration::from_secs(10))];
    for r in 0..rounds {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            entity(project),
            KEY_DEF,
            1_000 + r,
        ));
        events.push(keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            edge(project),
            KEY_REF,
            1_000 + r,
        ));
    }
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed the namespace");
}

fn prune_all_types(backend: &Store, prefix: &str) -> PrunedDerived {
    backend
        .prune_derived_index(prefix, &rigger::ingest::derived_index_identity())
        .expect("prune the derived index")
}

/// The SHIPPED derived-index policy with one field varied - the API-edge tests below have to drive
/// a metadata key nothing carries and a covered-type list the caller chose. They vary exactly that
/// field of the real policy (its key SPLIT comes straight off it), so no test ever stands up a
/// second, hand-written parser of the same key form to test the prune against.
///
/// The valid-time partition is re-declared over the varied type list, because the prune refuses a
/// declaration naming a type the policy does not cover: narrowing the covered types narrows the
/// declaration with it, which is exactly what a composition root varying this policy would have to
/// do. The membership is still the shipped answer, never a hand-written one.
fn identity_with(meta_key: &str, types: &[&str]) -> ContentIdentity {
    let shipped = rigger::ingest::derived_index_identity();
    let reasserting: Vec<&str> = rigger::ingest::reasserted_derived_types()
        .into_iter()
        .filter(|t| types.contains(t))
        .collect();
    ContentIdentity::new(meta_key, types.to_vec(), shipped.split())
        .with_reasserting_types(reasserting)
}

// ---------------------------------------------------------------------------------------
// 1. The namespace boundary: a store-level prune reaches EXACTLY one project's streams
// ---------------------------------------------------------------------------------------

#[test]
fn the_prune_reaches_only_the_namespace_it_was_handed_and_matches_that_prefix_literally() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("shared.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();

    // One backend file holding three projects - the shape `Namespaced` is documented to serve.
    // `my_repo` is the project being pruned and its identity carries a SQL wildcard (`_`);
    // `myXrepo` is the namespace a `LIKE`-based prefix match would sweep in with it (the global
    // read filter IS `LIKE`-based, so the two prefix comparisons in this store are deliberately
    // not the same one); `other` is an ordinary unrelated neighbour.
    const TARGET: &str = "my_repo";
    const WILDCARD_NEIGHBOUR: &str = "myXrepo";
    const NEIGHBOUR: &str = "other";
    const ROUNDS: u64 = 6;
    for project in [TARGET, WILDCARD_NEIGHBOUR, NEIGHBOUR] {
        seed_namespace(&backend, project, ROUNDS);
    }

    let before = raw_rows(&db);

    // The published prefix speaks for the decorator: the streams `Namespaced::new` actually wrote
    // are exactly the ones `prefix_for` addresses. Without this the maintenance path could drift
    // from the write path and prune a namespace nothing lives in - a silent no-op.
    for project in [TARGET, WILDCARD_NEIGHBOUR, NEIGHBOUR] {
        let expected = format!(
            "{}{}",
            Namespaced::prefix_for(project),
            rigger::conductor::STREAM
        );
        assert!(
            before.iter().any(|r| r.1 == expected),
            "Namespaced::new({project:?}) must write the stream Namespaced::prefix_for addresses \
             ({expected:?}); the log holds {:?}",
            before
                .iter()
                .map(|r| r.1.clone())
                .collect::<std::collections::BTreeSet<_>>()
        );
    }

    let pruned = prune_all_types(&backend, &Namespaced::prefix_for(TARGET));

    // Only the target's duplication is gone: two keys, each keeping its latest recording.
    assert_eq!(
        pruned.total_removed(),
        2 * (ROUNDS as usize - 1),
        "the prune must remove every superseded recording of the target's two keys, and nothing \
         beyond them; got {:?}",
        pruned.removed
    );

    let after = raw_rows(&db);
    let target_prefix = Namespaced::prefix_for(TARGET);
    for key in [KEY_DEF, KEY_REF] {
        let kept: Vec<Row> = rows_in(&after, &target_prefix)
            .into_iter()
            .filter(|r| replay_key(r).as_deref() == Some(key))
            .collect();
        assert_eq!(
            kept.len(),
            1,
            "exactly one recording of {key} must survive inside the pruned namespace"
        );
        let latest = rows_in(&before, &target_prefix)
            .into_iter()
            .filter(|r| replay_key(r).as_deref() == Some(key))
            .map(|r| r.0)
            .max()
            .expect("the seed recorded this key in the target namespace");
        assert_eq!(
            kept[0].0, latest,
            "the surviving recording of {key} must be the latest one the namespace held"
        );
    }

    // AND NOWHERE ELSE. Every other namespace is byte-for-byte identical, INCLUDING each row's
    // global position and per-stream revision: a prune that reached across the namespace boundary
    // would be destroying another project's history on a shared backend, which is the one failure
    // mode of this command an operator cannot undo.
    for project in [WILDCARD_NEIGHBOUR, NEIGHBOUR] {
        let prefix = Namespaced::prefix_for(project);
        let expected = rows_in(&before, &prefix);
        let actual = rows_in(&after, &prefix);
        assert!(
            !expected.is_empty(),
            "the seed must actually populate {project}, or the equality below proves nothing"
        );
        assert_eq!(
            actual, expected,
            "reset --derived on {TARGET} must leave {project}'s streams byte-for-byte untouched"
        );
    }

    // The same guarantee stated the other way round: the number of rows the file lost equals the
    // number the report claimed, so nothing was deleted that the report did not account for.
    assert_eq!(
        before.len() - after.len(),
        pruned.total_removed(),
        "the rows the file lost must be exactly the rows the prune reported removing"
    );
}

// ---------------------------------------------------------------------------------------
// 2. The API edges of the new store primitive
// ---------------------------------------------------------------------------------------

#[test]
fn prune_derived_index_keeps_the_callers_type_order_and_is_a_faithful_no_op_at_its_edges() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("edges.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    seed_namespace(&backend, "edges", 4);
    let seeded = raw_rows(&db);

    // ASKED FOR NOTHING. No type is eligible, so nothing is removed and nothing is reported -
    // an empty request is answered with an empty accounting, not with a guess at a default set.
    let none = backend
        .prune_derived_index("", &identity_with(rigger::ingest::META_REPLAY_KEY, &[]))
        .unwrap();
    assert!(
        none.removed.is_empty() && none.total_removed() == 0,
        "a prune asked for no types must report removing nothing; got {none:?}"
    );
    assert_eq!(
        raw_rows(&db),
        seeded,
        "a prune asked for no types must leave every row byte-for-byte, VACUUM included"
    );

    // A KEY NO ROW CARRIES. The metadata name is an authority the caller supplies; pointed at a
    // name nothing is keyed by, every row is keyless and therefore never provably redundant, so
    // the prune removes nothing. This is the fail-safe direction at the API edge: a caller that
    // passed the wrong key name loses no data.
    let wrong_key = backend
        .prune_derived_index(
            &Namespaced::prefix_for("edges"),
            &identity_with("no_such_metadata_key", &rigger::ingest::DERIVED_INDEX_TYPES),
        )
        .unwrap();
    assert_eq!(
        wrong_key.total_removed(),
        0,
        "a prune keyed on metadata no row carries must remove nothing; got {wrong_key:?}"
    );
    assert_eq!(
        raw_rows(&db),
        seeded,
        "a prune keyed on metadata no row carries must leave every row untouched"
    );

    // A NAMESPACE THAT DOES NOT EXIST. Same fail-safe: an identity with no streams prunes nothing
    // rather than falling back to the whole file.
    let absent = prune_all_types(&backend, &Namespaced::prefix_for("no-such-project"));
    assert_eq!(
        absent.total_removed(),
        0,
        "a prune of an unpopulated namespace must remove nothing; got {absent:?}"
    );
    assert_eq!(
        raw_rows(&db),
        seeded,
        "a prune of an unpopulated namespace must leave every row untouched"
    );

    // THE REPORT IS THE CALLER'S LIST. Every type the caller named appears exactly once, in the
    // order it was named, including the ones nothing was removed from - that ordering is what
    // lets an operator (or a test) read the report positionally against what they asked for.
    let mut reversed: Vec<&str> = rigger::ingest::DERIVED_INDEX_TYPES.to_vec();
    reversed.reverse();
    let report = backend
        .prune_derived_index(
            &Namespaced::prefix_for("edges"),
            &identity_with(rigger::ingest::META_REPLAY_KEY, &reversed),
        )
        .unwrap();
    assert_eq!(
        report
            .removed
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>(),
        reversed,
        "the per-type report must list exactly the types the caller named, in the caller's order"
    );
    assert_eq!(
        report.total_removed(),
        report.removed.iter().map(|(_, n)| n).sum::<usize>(),
        "the total must be the sum of the per-type counts it summarizes"
    );

    // The zero value of the report is a coherent empty accounting, so a caller can construct and
    // compare one without a prune having run.
    let empty = PrunedDerived::default();
    assert_eq!(empty.total_removed(), 0);
    assert!(empty.removed.is_empty() && empty.reclaimed_bytes.is_none());
}

/// The valid-time partition is an input the prune CANNOT default, so it refuses instead.
///
/// Both refusals guard a corruption that leaves every row looking intact. A policy that never
/// declared the partition cannot be read as "nothing re-asserts": the deletes run over every
/// covered type either way, so an unnamed re-asserting type would lose its earliest recordings
/// with no carry and have every one of its facts silently re-dated to whichever recording
/// survived. And a declaration naming a type the policy does not cover describes some OTHER
/// policy, so it cannot be this one's partition - taking it anyway would let a caller believe it
/// had declared a carry that the prune, which iterates its own covered types, never performs.
///
/// The store is required to say so BEFORE it takes the write lock, because a refusal that arrives
/// after a partial prune is not a refusal.
#[test]
fn a_prune_whose_policy_never_declared_the_valid_time_partition_is_refused_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("undeclared.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    seed_namespace(&backend, "undeclared", 4);
    let seeded = raw_rows(&db);
    let prefix = Namespaced::prefix_for("undeclared");

    // The shipped policy WITHOUT its declaration: everything else about it is the real thing, so
    // the only reason to refuse is the missing partition.
    let shipped = rigger::ingest::derived_index_identity();
    let undeclared = ContentIdentity::new(
        rigger::ingest::META_REPLAY_KEY,
        rigger::ingest::DERIVED_INDEX_TYPES,
        shipped.split(),
    );
    assert!(
        undeclared.reasserting().is_none() && shipped.reasserting().is_some(),
        "the fixture must differ from the shipped policy in exactly the declaration"
    );
    let refused = backend
        .prune_derived_index(&prefix, &undeclared)
        .expect_err("a policy with no declared partition must be refused, not guessed at");
    let said = refused.to_string();
    for needle in ["re-assert", "with_reasserting_types"] {
        assert!(
            said.contains(needle),
            "the refusal must name what was not declared and how to declare it ({needle:?}); got \
             {said:?}"
        );
    }

    // A DECLARATION ABOUT ANOTHER POLICY is refused the same way.
    let stray = ContentIdentity::new(
        rigger::ingest::META_REPLAY_KEY,
        rigger::ingest::DERIVED_INDEX_TYPES,
        shipped.split(),
    )
    .with_reasserting_types(["ReviewFinding"]);
    let refused = backend
        .prune_derived_index(&prefix, &stray)
        .expect_err("a declaration naming an uncovered type must be refused");
    assert!(
        refused.to_string().contains("ReviewFinding"),
        "the refusal must name the type that does not belong to this policy; got {refused:?}"
    );

    // NEITHER REFUSAL TOUCHED THE LOG. A refusal that has already deleted rows is not a refusal,
    // and this is the assertion that the checks run before the transaction rather than inside it.
    assert_eq!(
        raw_rows(&db),
        seeded,
        "a refused prune must leave every row byte-for-byte, VACUUM included"
    );

    // AND AN EMPTY DECLARATION IS NOT THE UNDECLARED STATE. It is a caller stating that none of
    // its types re-assert, which is a thing a caller may truthfully say, so it prunes.
    let declared_empty = ContentIdentity::new(
        rigger::ingest::META_REPLAY_KEY,
        rigger::ingest::DERIVED_INDEX_TYPES,
        shipped.split(),
    )
    .with_reasserting_types(Vec::<String>::new());
    let pruned = backend
        .prune_derived_index(&prefix, &declared_empty)
        .expect("an empty declaration is a declaration and must be honored");
    assert!(
        pruned.total_removed() > 0,
        "the seeded log holds duplicates, so an honored prune must shed them; got {pruned:?}"
    );
}

/// The reported reclamation is a MEASUREMENT OF THE FILE, so when the file did not shrink the
/// report must not say it did.
///
/// `PRAGMA wal_checkpoint(TRUNCATE)` is what folds the freed pages out of the write-ahead log and
/// back into `events.db`, and it DECLINES while any reader still holds a snapshot of that log -
/// its first result column is the BUSY flag. Read only the page-count delta and the two cases are
/// indistinguishable: the prune reports a healthy byte count while `events.db` is unchanged on
/// disk and total bytes have gone UP, which is the one report an operator has no way to check. So
/// the contended case is asserted directly, with a reader deliberately parked on a snapshot.
///
/// The deletion itself is unaffected and that is asserted too - the transaction committed before
/// any of this - so the test says "the rows went, the reclamation is unmeasured" rather than
/// letting a failure to prune masquerade as a failure to measure.
#[test]
fn a_reader_holding_the_write_ahead_log_makes_the_reclamation_unmeasured_not_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("contended.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    seed_namespace(&backend, "contended", 6);
    let prefix = Namespaced::prefix_for("contended");

    // UNCONTENDED FIRST, on an identical seed, so the two reports are compared rather than one
    // being asserted in isolation: whatever this one reports, it is a number.
    let solo_db = dir.path().join("uncontended.db");
    let solo = Store::open(solo_db.to_str().unwrap()).unwrap();
    seed_namespace(&solo, "contended", 6);
    // ROOM TO RECLAIM on both logs. The rewrite runs over a file holding reclaimable free pages,
    // and the seeded duplication is a handful of small rows that can free no whole page - so
    // without this both prunes would honestly skip the rewrite and the checkpoint this test is
    // about would never be asked for.
    plant_free_pages(&db, 3_000);
    plant_free_pages(&solo_db, 3_000);
    let solo_report = solo
        .prune_derived_index(&prefix, &rigger::ingest::derived_index_identity())
        .expect("prune the uncontended log");
    assert!(
        solo_report.reclaimed_bytes.is_some() && solo_report.total_removed() > 0,
        "with no reader parked on the log the checkpoint completes, so the reclamation is a \
         measurement; got {solo_report:?}"
    );

    // NOW PARK A READER ON THE WRITE-AHEAD LOG. An open read transaction is exactly the state that
    // makes a truncating checkpoint decline, and it is an ordinary thing for a second rigger
    // process to be doing.
    let reader = rusqlite::Connection::open(&db).expect("open a second connection");
    reader
        .execute_batch("BEGIN")
        .expect("begin the reader's transaction");
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("take a read snapshot");

    let report = backend
        .prune_derived_index(&prefix, &rigger::ingest::derived_index_identity())
        .expect("prune the contended log");
    assert_eq!(
        report.total_removed(),
        solo_report.total_removed(),
        "a parked reader must not change WHAT the prune deletes - the deletes commit before the \
         checkpoint is ever asked for"
    );
    assert_eq!(
        report.reclaimed_bytes, None,
        "the checkpoint was refused, so the freed pages are still in the write-ahead log and the \
         file did not shrink: the reclamation is UNMEASURED, never a byte count the operator's own \
         `ls` would contradict; got {report:?}"
    );

    drop(reader);
}

// ---------------------------------------------------------------------------------------
// 3. The cross-module seam: the command prunes what `ingest` declares, and nothing else decides
// ---------------------------------------------------------------------------------------

/// A throwaway project whose identity resolves the same way it does for a real one.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    std::fs::create_dir_all(dir.path().join(".rigger")).expect("create .rigger");
    dir
}

fn event_log(root: &Path) -> PathBuf {
    root.join(".rigger").join("events.db")
}

/// The project identity the binary resolves for `root`, mirrored here so a seed lands in the very
/// stream the compiled binary reads back.
fn project_identity(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// Run `rigger <args...>` in `cwd`. The dashboard and the machine-global instance registry are
/// stubbed out so a short-lived invocation leaves no live process or phantom registry entry.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = Command::new(common::rigger_bin());
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn seed_project(root: &Path, rounds: u64) {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    seed_namespace(&backend, &project_identity(root), rounds);
}

/// The `(type, count)` pairs out of the command's report - the parenthesised list in
/// `pruned N ... from the event log (<type> <n>, ...)`.
fn per_type_report(out: &str) -> Vec<(String, usize)> {
    let marker = "from the event log (";
    let at = out
        .find(marker)
        .unwrap_or_else(|| panic!("the report must carry a per-type list; got {out:?}"))
        + marker.len();
    let rest = &out[at..];
    let end = rest
        .find(')')
        .unwrap_or_else(|| panic!("the per-type list must be closed; got {out:?}"));
    rest[..end]
        .split(", ")
        .map(|item| {
            let (name, n) = item
                .rsplit_once(' ')
                .unwrap_or_else(|| panic!("each entry must be `<type> <count>`; got {item:?}"));
            (
                name.to_string(),
                n.parse::<usize>()
                    .unwrap_or_else(|_| panic!("each entry must end in a count; got {item:?}")),
            )
        })
        .collect()
}

#[test]
fn the_command_prunes_and_accounts_for_exactly_the_derived_index_types_ingest_declares() {
    let dir = temp_project();
    let root = dir.path();
    const ROUNDS: u64 = 5;
    seed_project(root, ROUNDS);
    let before = raw_rows(&event_log(root)).len();

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");

    // THE SEAM. `rigger::ingest` owns the list of derived index types; the command must prune and
    // account for that list, not a copy of it. Declaring a fifth derived type in `ingest` then
    // has exactly one honest outcome here - the report grows a fifth entry - instead of a type
    // that quietly accumulates forever because the prune never heard about it.
    let report = per_type_report(&out);
    assert_eq!(
        report.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(),
        rigger::ingest::DERIVED_INDEX_TYPES.to_vec(),
        "the command must account for every type ingest declares derived, in that order; got \
         {out:?}"
    );

    // And the account is TRUE: the per-type counts sum to the headline number, and that number is
    // exactly how many rows the file actually lost.
    let summed: usize = report.iter().map(|(_, n)| n).sum();
    let after = raw_rows(&event_log(root)).len();
    assert_eq!(
        before - after,
        summed,
        "the per-type counts must add up to the rows the log actually lost; got {out:?}"
    );
    assert_eq!(
        summed,
        2 * (ROUNDS as usize - 1),
        "the seeded duplication of both keys must be what was pruned; got {out:?}"
    );
}

// ---------------------------------------------------------------------------------------
// 4. The event-store contract on a compacted stream
// ---------------------------------------------------------------------------------------

#[test]
fn a_compacted_stream_answers_expected_revision_from_its_highest_surviving_revision() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("cursor.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    const PROJECT: &str = "cursor";

    // revision 0 is non-derived, revisions 1..=6 are six recordings of ONE key, revision 7 is
    // non-derived again. The prune therefore deletes from the MIDDLE of the stream, which is the
    // only shape that separates `MAX(revision)` from `COUNT(*) - 1`.
    {
        let store = Namespaced::new(&backend, PROJECT);
        let mut events = vec![Event::new("RunStarted", br#"{"run":"r1"}"#.to_vec())];
        for r in 0..6u64 {
            events.push(keyed(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                entity("alpha"),
                KEY_DEF,
                1_000 + r,
            ));
        }
        events.push(Event::new("RunFinished", br#"{"run":"r1"}"#.to_vec()));
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
            .unwrap();
    }

    let pruned = prune_all_types(&backend, &Namespaced::prefix_for(PROJECT));
    assert_eq!(
        pruned.total_removed(),
        5,
        "the five superseded recordings must be the rows removed; got {pruned:?}"
    );

    // THE SURVIVORS KEEP THEIR NUMBERS. Closing the holes would mean REWRITING events the
    // compaction is only allowed to preserve, so the revision sequence is left gapped on purpose:
    // 0, 6, 7. Three rows, whose highest revision is 7 - the two numbers that must not be
    // confused.
    let revisions: Vec<i64> = raw_rows(&db).iter().map(|r| r.6).collect();
    assert_eq!(
        revisions,
        vec![0, 6, 7],
        "the compaction must move the CURSOR, never renumber the rows it preserved"
    );

    let store = Namespaced::new(&backend, PROJECT);
    let probe = |type_: &str| [Event::new(type_, b"{}".to_vec())];

    // The stale, count-derived cursor (rows - 1 = 2) must be REFUSED, naming the real revision.
    // A store that still derived its cursor from the row count would accept this and then collide
    // on `UNIQUE(stream, revision)` - or worse, silently reissue a revision the stream holds.
    match store.append(
        rigger::conductor::STREAM,
        ExpectedRevision::Exact(2),
        &probe("Stale"),
    ) {
        Err(Error::Conflict { actual, .. }) => assert_eq!(
            actual, 7,
            "the conflict must report the stream's REAL current revision"
        ),
        other => panic!(
            "Exact(2) must conflict on a compacted stream whose max revision is 7; got {other:?}"
        ),
    }

    // A compacted stream is still a stream that EXISTS: pruning rows out of its middle must never
    // make it look empty to the create-only guard.
    match store.append(
        rigger::conductor::STREAM,
        ExpectedRevision::NoStream,
        &probe("Ghost"),
    ) {
        Err(Error::Conflict { actual, .. }) => assert_eq!(actual, 7),
        other => panic!("NoStream must conflict on a compacted, non-empty stream; got {other:?}"),
    }

    // And the true cursor is accepted, placing the next event one past the highest survivor.
    store
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Exact(7),
            &probe("RunStarted"),
        )
        .expect("Exact(max revision) must be the cursor a compacted stream accepts");

    let log = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert_eq!(
        log.iter().map(|e| e.revision).collect::<Vec<_>>(),
        vec![0, 6, 7, 8],
        "the appended event must take the revision after the highest survivor, and the gaps must \
         still read back cleanly"
    );

    // Reading from a revision INSIDE a gap is still well defined: revisions order the stream, so
    // `from` is a lower bound, not an index into a dense sequence.
    let tail = store
        .read_stream(rigger::conductor::STREAM, 3, Direction::Forward)
        .unwrap();
    assert_eq!(
        tail.iter().map(|e| e.revision).collect::<Vec<_>>(),
        vec![6, 7, 8],
        "reading from a revision that was pruned away must return the survivors above it"
    );
}

// ---------------------------------------------------------------------------------------
// 5. The command's flag registry, at the edges the second mode opened
// ---------------------------------------------------------------------------------------

#[test]
fn reset_accepts_each_mode_at_most_once_and_composes_the_two_in_either_order() {
    let dir = temp_project();
    let root = dir.path();
    seed_project(root, 3);

    // A repeated mode is a typed mistake, not an instruction to prune twice: it is refused, and
    // the refusal names the flag that was repeated so the operator can see which one it was.
    for flag in ["--derived", "--runs"] {
        let (out, err, ok) = run_rigger(root, &["reset", flag, flag]);
        assert!(!ok, "reset {flag} {flag} must be refused; stdout: {out}");
        let said = format!("{err}{out}");
        assert!(
            said.contains(flag) && said.contains("more than once"),
            "the refusal must name the repeated flag; got {said:?}"
        );
    }

    // The modes are named, not positional: composing them the other way round does the same two
    // prunes. Each still reports its own work, so neither silently swallows the other.
    let (out, err, ok) = run_rigger(root, &["reset", "--derived", "--runs"]);
    assert!(
        ok,
        "reset --derived --runs must succeed; stderr: {err}\n{out}"
    );
    assert!(
        out.contains("reset --runs:") && out.contains("reset --derived:"),
        "a composed reset must report BOTH prunes whichever order they were named in; got {out:?}"
    );
    assert_eq!(
        per_type_report(&out)
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>(),
        rigger::ingest::DERIVED_INDEX_TYPES.to_vec(),
        "the composed --derived prune must render the same full accounting it does alone"
    );
}

// ---------------------------------------------------------------------------------------
// 6. The namespace boundary the whole store-MAINTENANCE family shares
// ---------------------------------------------------------------------------------------

/// `Namespaced::prefix_for` is published with a claim that reaches past the prune that needed it:
/// it is the ONE place the namespace's wire form is written, so that store maintenance and every
/// namespaced read and write agree on the same boundary, and a change to the form can never leave
/// a maintenance command addressing streams that no longer exist.
///
/// Three prefix-taking maintenance ops now sit on the concrete store - `has_stream_prefix`,
/// `rename_stream_prefix` (the project-identity migration) and this unit's `prune_derived_index` -
/// and nothing holds them to that claim by construction: the migration still spells the form at
/// its own call site. So the PROPERTY the claim is worth is pinned here rather than assumed, at
/// the one place an operator would ever notice it break: a project whose streams the identity
/// migration MOVED must still be seen, and still be compacted, at its new identity.
///
/// If the prune and the migration ever spoke different boundary languages, a migrated project's
/// log would silently stop compacting - `reset --derived` would report a clean zero forever while
/// the duplication it exists to shed kept growing under a name it no longer addresses. That is the
/// one failure of this command an operator cannot see, because a prune that removes nothing and a
/// log that holds nothing redundant print the same report.
#[test]
fn a_migrated_project_log_is_still_seen_and_compacted_at_its_new_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("migrated.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();

    const LEGACY: &str = "legacy-id";
    const MINTED: &str = "minted-id";
    const NEIGHBOUR: &str = "other";
    const ROUNDS: u64 = 4;
    seed_namespace(&backend, LEGACY, ROUNDS);
    seed_namespace(&backend, NEIGHBOUR, ROUNDS);

    let legacy_ns = Namespaced::prefix_for(LEGACY);
    let minted_ns = Namespaced::prefix_for(MINTED);
    let before = raw_rows(&db);
    let legacy_rows = rows_in(&before, &legacy_ns);
    assert!(
        !legacy_rows.is_empty(),
        "the seed must populate the legacy namespace, or nothing below proves anything"
    );

    // The presence probe and the write path already agree: the streams `Namespaced::new` wrote are
    // the ones `has_stream_prefix` finds at `prefix_for`, and an identity nothing was written under
    // is absent rather than incidentally matched.
    assert!(
        backend.has_stream_prefix(&legacy_ns).unwrap(),
        "has_stream_prefix must see the streams Namespaced::new wrote at prefix_for({LEGACY:?})"
    );
    assert!(
        !backend.has_stream_prefix(&minted_ns).unwrap(),
        "nothing has been written under prefix_for({MINTED:?}) yet"
    );

    // THE MIGRATION MOVES THE STREAMS - between the same two boundaries the prune speaks.
    let renamed = backend
        .rename_stream_prefix(&legacy_ns, &minted_ns)
        .expect("rename the project's streams onto its minted identity");
    assert_eq!(
        renamed, 1,
        "the seed writes one stream per namespace, so exactly one is moved"
    );
    assert!(
        !backend.has_stream_prefix(&legacy_ns).unwrap(),
        "the legacy namespace must be empty once the migration has moved its streams"
    );
    assert!(
        backend.has_stream_prefix(&minted_ns).unwrap(),
        "the minted namespace must hold the moved streams"
    );

    // The migration MOVED rows, it did not rewrite them: same positions, same revisions, same
    // bytes, only the stream's prefix differs. The prune partitions BY STREAM, so this is the
    // precondition under which its per-stream reasoning still holds after a migration.
    let after_rename = raw_rows(&db);
    let moved = rows_in(&after_rename, &minted_ns);
    assert_eq!(
        moved.len(),
        legacy_rows.len(),
        "every row must survive the move"
    );
    for (was, now) in legacy_rows.iter().zip(moved.iter()) {
        assert_eq!(
            (was.0, &was.2, &was.3, &was.4, &was.5, was.6),
            (now.0, &now.2, &now.3, &now.4, &now.5, now.6),
            "the migration must move a row without renumbering or rewriting it"
        );
        assert_eq!(
            now.1,
            was.1.replacen(&legacy_ns, &minted_ns, 1),
            "only the namespace prefix of the stream name may change"
        );
    }

    // THE PRUNE FOLLOWS THE MIGRATION. At the OLD identity there is nothing left to compact, and
    // the fail-safe direction holds: an empty namespace is a no-op, never a fallback to the file.
    let stale = prune_all_types(&backend, &legacy_ns);
    assert_eq!(
        stale.total_removed(),
        0,
        "a prune at the identity the project no longer uses must remove nothing; got {stale:?}"
    );
    assert_eq!(
        raw_rows(&db),
        after_rename,
        "a prune at the vacated identity must leave every row untouched"
    );

    // At the NEW identity the log is compactable exactly as it was before it moved: both keys keep
    // their latest recording and every superseded one goes.
    let pruned = prune_all_types(&backend, &minted_ns);
    assert_eq!(
        pruned.total_removed(),
        2 * (ROUNDS as usize - 1),
        "a migrated log must still shed the duplication of both keys; got {pruned:?}"
    );

    let after = raw_rows(&db);
    for key in [KEY_DEF, KEY_REF] {
        let kept: Vec<Row> = rows_in(&after, &minted_ns)
            .into_iter()
            .filter(|r| replay_key(r).as_deref() == Some(key))
            .collect();
        assert_eq!(
            kept.len(),
            1,
            "exactly one recording of {key} must survive at the minted identity"
        );
        let latest = moved
            .iter()
            .filter(|r| replay_key(r).as_deref() == Some(key))
            .map(|r| r.0)
            .max()
            .expect("the moved namespace holds this key");
        assert_eq!(
            kept[0].0, latest,
            "the survivor must be the latest recording the moved namespace held"
        );
    }

    // And the neighbour that never migrated is byte-for-byte untouched by either op.
    let neighbour_ns = Namespaced::prefix_for(NEIGHBOUR);
    assert_eq!(
        rows_in(&after, &neighbour_ns),
        rows_in(&before, &neighbour_ns),
        "neither the migration nor the prune may reach a namespace it was not handed"
    );
}

// ---------------------------------------------------------------------------------------
// 7. The shipped operator-facing artifacts: the usage registry and the committed documents
// ---------------------------------------------------------------------------------------

/// The `reset` modes the binary's usage registry ADVERTISES, read out of the help it actually
/// prints. Deriving the set from the running binary rather than naming it here is what makes a
/// third mode covered by the assertions below without anyone editing this test.
fn advertised_reset_modes(help: &str) -> Vec<String> {
    let mut modes = BTreeSet::new();
    for line in help.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("rigger reset ") {
            if let Some(mode) = rest.split_whitespace().next() {
                if let Some(stripped) = mode.strip_prefix("--") {
                    modes.insert(format!("--{stripped}"));
                }
            }
        }
    }
    modes.into_iter().collect()
}

/// The registry entry for one `rigger reset <mode>`: its own line and the wrapped continuation
/// lines under it, up to whatever entry comes next.
fn registry_entry(help: &str, mode: &str) -> String {
    let opener = format!("rigger reset {mode}");
    let mut out: Vec<&str> = Vec::new();
    for line in help.lines() {
        let trimmed = line.trim_start();
        if out.is_empty() {
            if trimmed.starts_with(&opener) {
                out.push(trimmed);
            }
            continue;
        }
        if trimmed.starts_with("rigger ") {
            break;
        }
        out.push(trimmed);
    }
    assert!(
        !out.is_empty(),
        "the usage registry must carry an entry for `{opener}`; got {help:?}"
    );
    out.join(" ")
}

/// The usage registry is the only description of `reset --derived` an operator gets from the
/// binary itself, and nothing in the crate asserted it before this unit added a second mode to a
/// command that had exactly one.
///
/// Two directions, both driven against the built binary:
///
///   - Every mode the registry ADVERTISES is a mode the parser accepts and actually runs. A
///     registry that documents a flag the binary refuses is worse than no documentation, because
///     it is the operator's evidence that they typed the right thing.
///   - A mode the registry does NOT advertise is refused, and the refusal names every advertised
///     mode - so the parser's own error text and the registry cannot drift into disagreeing about
///     what this command takes.
///
/// The `--derived` entry is additionally held to the two facts that decide whether an operator
/// reaches for it at all: WHICH store it compacts (the event log, not the graph - they are
/// different piles with different prunes) and that it COMPOSES with `--runs` rather than replacing
/// it.
#[test]
fn the_usage_registry_advertises_the_derived_prune_and_every_mode_it_advertises_is_real() {
    let dir = temp_project();
    let (out, err, ok) = run_rigger(dir.path(), &["--help"]);
    assert!(ok, "rigger --help must succeed; stderr: {err}\n{out}");
    let help = format!("{err}{out}");

    let modes = advertised_reset_modes(&help);
    assert!(
        modes.iter().any(|m| m == "--derived"),
        "the usage registry must advertise the mode this unit shipped; it advertises {modes:?}"
    );

    let derived = registry_entry(&help, "--derived");
    assert!(
        derived.contains("EVENT LOG"),
        "the --derived entry must say WHICH store it compacts, since --runs prunes a different \
         one; got {derived:?}"
    );
    assert!(
        derived.contains("--runs"),
        "the --derived entry must say it composes with the mode that was already there; got \
         {derived:?}"
    );

    // EVERY ADVERTISED MODE IS REAL. Each runs in its own freshly seeded project so one mode's
    // prune cannot be what makes the next one look like it worked.
    for mode in &modes {
        let project = temp_project();
        seed_project(project.path(), 3);
        let (out, err, ok) = run_rigger(project.path(), &["reset", mode]);
        let said = format!("{err}{out}");
        assert!(
            ok,
            "the registry advertises `rigger reset {mode}`, so the binary must accept it; got \
             {said:?}"
        );
        assert!(
            !said.contains("expected --runs and/or --derived"),
            "`rigger reset {mode}` is advertised, so it must not be refused as unrecognized; got \
             {said:?}"
        );
    }

    // AND NOTHING ELSE IS. An unadvertised mode is refused, and the refusal enumerates exactly the
    // modes the registry advertises, so a mode added to one and not the other cannot go unnoticed.
    let project = temp_project();
    seed_project(project.path(), 3);
    let (out, err, ok) = run_rigger(project.path(), &["reset", "--everything"]);
    let said = format!("{err}{out}");
    assert!(
        !ok,
        "an unadvertised reset mode must be refused; got {said:?}"
    );
    for mode in &modes {
        assert!(
            said.contains(mode.as_str()),
            "the refusal must name every mode the registry advertises ({modes:?}), so the two \
             cannot disagree about what reset takes; got {said:?}"
        );
    }
}

/// The two documents this unit re-rendered, at the path they ship from.
const SHIPPED_DOCS: [&str; 2] = [
    "skills/using-rigger/SKILL.md",
    "docs/handbook/using-rigger.md",
];

/// The COMMITTED operator guidance, asserted on the bytes on disk with no render in the loop.
///
/// This is not the render test in `src/docs.rs` restated. That test renders `discipline_body` from
/// a SENTINEL context (a placeholder base ref, port 65531, invented subcommand names); these two
/// files are rendered from the REAL one. "The sentinel render carries the paragraph" and "the
/// committed file equals a fresh real render" together still do not give "the committed file
/// carries the paragraph" - any context-conditional branch in the body satisfies both while the
/// shipped document says nothing. This unit has already failed at exactly this boundary once: the
/// body gained the `--derived` paragraph, the render test went green, and both shipped documents
/// stayed at their pre-change text, so the guidance for a command that deletes from an append-only
/// log was not actually shipped to anyone.
///
/// So the shipped bytes are read directly and held to the things an operator must know before
/// running it - WHAT IT KEEPS, WHAT IT COSTS (both what the other events cost, which is nothing,
/// and what the rewrite costs, which is a full copy of the log on a partition they are probably
/// not watching), that the file shrinks, WHAT IT CANNOT RECLAIM, the one case in which a
/// deduplicated log still has rows to shed, and that the two prunes compose - plus the proof that
/// these documents were rendered from the real context at all.
///
/// AND THEN TO THE RENDERER'S OWN TEXT, WORD FOR WORD, which is the assertion that can actually go
/// red on staleness. A list of substrings cannot: a uniformly stale render satisfies every one of
/// them, and so does a stale render that agrees with its equally stale sibling, so the checks above
/// pin that the guidance EXISTS and this one pins that it is the CURRENT guidance. Any later
/// sentence added to the renderer is therefore shipped or the test fails - including one that
/// changes what the paragraph claims. It doubles as the proof that this paragraph is not
/// context-conditional: it is compared against a render from a context that shares no value with
/// the real one, so a paragraph that varied with the context could not match.
#[test]
fn the_committed_operator_documents_ship_the_derived_prunes_guidance() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paragraphs: Vec<(String, String)> = Vec::new();

    for rel in SHIPPED_DOCS {
        let path = manifest.join(rel);
        let shipped = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "the operator document {rel} must ship from {}: {e}",
                path.display()
            )
        });

        for (fact, needle) in [
            ("name the prune", "rigger reset --derived"),
            ("say which store it compacts", "EVENT LOG"),
            ("say what it KEEPS", "LATEST event per replay key"),
            ("say what it costs everything else", "byte-for-byte"),
            ("say the file actually shrinks", "shrinks on disk"),
            ("say what it CANNOT reclaim", "WHAT IT CANNOT RECLAIM"),
            (
                "say when a DEDUPLICATED log still prunes rows",
                "RETURNED to a generation the log had already recorded",
            ),
            (
                "say where the compaction stages its copy of the log",
                "temporary directory",
            ),
            (
                "say a prune with nothing to shed does not rewrite the file",
                "leaves the file exactly as it found it",
            ),
            (
                "show the two prunes composing",
                "rigger reset --runs --derived",
            ),
        ] {
            assert!(
                shipped.contains(needle),
                "the committed {rel} must {fact} ({needle:?}); an operator reads this file, not a \
                 fresh render of it"
            );
        }

        // The document was rendered from the REAL context, not the sentinel one the render test
        // uses: the dashboard address it quotes is the port the code actually defaults to.
        assert!(
            shipped.contains(&format!("127.0.0.1:{}", rigger::dash::DEFAULT_PORT)),
            "the committed {rel} must be a render of the real context (dash port {})",
            rigger::dash::DEFAULT_PORT
        );

        paragraphs.push((rel.to_string(), derived_paragraph(rel, &shipped)));
    }

    // ONE body, two consumers: the skill and the handbook chapter render from the same
    // `discipline_body`, so the paragraph an operator reads must be the same one whichever
    // document they opened. Comparing the shipped text is what proves it for the files that
    // actually ship, rather than for the renderer.
    let (first_rel, first) = &paragraphs[0];
    for (rel, paragraph) in &paragraphs[1..] {
        assert_eq!(
            paragraph, first,
            "{rel} and {first_rel} render from one shared discipline body, so their --derived \
             guidance must not have drifted apart"
        );
    }

    // AND IT IS THE CURRENT PARAGRAPH, not merely a paragraph the two files agree on. Two stale
    // files agree with each other perfectly; the renderer is the only thing that can tell a
    // shipped document from a stale one. `rigger docs` re-renders these files from the binary
    // built out of this tree, so an edit to `discipline_body` that was not re-rendered and
    // committed - or was re-rendered by an OLDER installed binary, which is how this unit lost
    // the paragraph once already - fails here with the two texts printed side by side.
    let rendered = derived_paragraph(
        "<discipline_body>",
        &rigger::docs::render_handbook_discipline(&docs_context_for_paragraph_comparison()),
    );
    for (rel, paragraph) in &paragraphs {
        assert_eq!(
            paragraph, &rendered,
            "the committed {rel} carries a STALE `--derived` paragraph: it is not what \
             `rigger::docs` renders today. Re-render it with a `rigger` built from THIS tree \
             (`cargo build` and run that binary's `docs`, or `cargo install --path .` first) and \
             commit the result"
        );
    }
}

/// The ONE `rigger reset --derived` paragraph in a rendered or shipped discipline document.
///
/// Exactly one, asserted rather than assumed: taking the first of several would let a second
/// mention elsewhere in the document silently become what the comparison above is about.
fn derived_paragraph(what: &str, text: &str) -> String {
    let mut found: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("rigger reset --derived"))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{what} must carry exactly one `rigger reset --derived` paragraph; got {}: {found:?}",
        found.len()
    );
    found.remove(0).to_string()
}

/// A docs context that shares NO value with the real one the shipped documents were rendered from.
///
/// That is the point of it. The paragraph this test compares must be the same text whatever the
/// context says, so rendering it from a context whose every field is deliberately unlike the real
/// one turns "these two paragraphs are equal" into a proof that the paragraph is unconditional -
/// which is exactly the hole a comparison against the real context would leave open.
fn docs_context_for_paragraph_comparison() -> rigger::docs::DocsContext {
    rigger::docs::DocsContext {
        base_ref: "not-the-real-base-ref".into(),
        dash_port: 65531,
        max_retries: 999,
        verdict_approve: "not-the-real-verdict".into(),
        spec_shape_rules: vec!["not-a-real-rule".into()],
        spec_shape_recommendation: "not the real recommendation".into(),
        subcommands: vec!["not-a-real-subcommand".into()],
        specs_location: "not/the/real/specs".into(),
    }
}

// ---------------------------------------------------------------------------------------
// 8. Two piles, two prunes: each sheds ONLY its own, and composing them does exactly both
// ---------------------------------------------------------------------------------------

/// The identity every fixture in this section is pinned to, so projects living under different
/// temp directories resolve to the SAME namespace and their stores compare row for row.
const PINNED_ID: &str = "compaction-fixture";

/// The decision the DEAD run recorded. It is the graph's half of the fixture: `--runs` must drop
/// its node, which is what makes "the graph changed" a fact rather than an assumption.
const DEAD_DECISION: &str = "d-dead-run";

/// A temp project whose identity is PINNED in `.rigger/project.id` - the first rung the binary's
/// identity resolution reads, and the one [`project_identity`] mirrors. Without it each fixture
/// would take its identity from its own temp directory name, so two identically-seeded projects
/// would write their events under two different stream names and could not be compared.
fn pinned_project() -> tempfile::TempDir {
    let dir = temp_project();
    std::fs::write(dir.path().join(".rigger").join("project.id"), PINNED_ID)
        .expect("pin the project identity");
    dir
}

fn graph_db(root: &Path) -> PathBuf {
    root.join(".rigger").join("graph.db")
}

/// The context graph's LIVE content: its nodes, and the edges that have not been retired. This is
/// what "the live graph is unchanged" has to mean - the file itself is rebuilt by the `--runs` vacuum,
/// so bytes on disk would answer the wrong question.
fn graph_rows(db: &Path) -> (Vec<String>, Vec<String>) {
    let conn = rusqlite::Connection::open(db).expect("open the context graph");
    let mut nodes: Vec<String> = conn
        .prepare("SELECT id, kind, COALESCE(attrs,''), project FROM nodes")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    nodes.sort();
    let mut edges: Vec<String> = conn
        .prepare("SELECT from_id, to_id, rel, project, tier FROM edges WHERE valid_to IS NULL")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    edges.sort();
    (nodes, edges)
}

/// A row with its event id dropped. Ids are minted per event, so two identically-seeded projects
/// agree on every column BUT that one; comparing their logs is a comparison of this shape.
fn shape(rows: &[Row]) -> Vec<(i64, String, String, Vec<u8>, String, i64)> {
    rows.iter()
        .map(|r| (r.0, r.1.clone(), r.2.clone(), r.4.clone(), r.5.clone(), r.6))
        .collect()
}

/// Where two row lists first part company, named compactly: `position/type/revision` on each side.
/// The comparisons below stay EXACT (whole rows, payload bytes included) - this only decides what a
/// failure prints, because a raw dump of two logs is unreadable in a gate log.
fn first_difference<T: PartialEq + std::fmt::Debug>(left: &[T], right: &[T]) -> String {
    for (i, (l, r)) in left.iter().zip(right.iter()).enumerate() {
        if l != r {
            return format!("index {i}: {l:?} vs {r:?}");
        }
    }
    format!("lengths {} vs {}", left.len(), right.len())
}

/// The compact form of a row a failure names: position, type, and per-stream revision.
fn row_marks(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|r| format!("{}/{}/{}", r.0, r.2, r.6))
        .collect()
}

/// Seed BOTH of the project's stores from ONE trajectory, the way a real project accumulates them:
/// a dead run that recorded a decision, the active run that superseded it, and `rounds`
/// re-recordings of two derived keys - then fold the log as it was WRITTEN into `.rigger/graph.db`.
///
/// Each prune therefore has its own pile waiting: the graph holds a dead run's node for `--runs`,
/// the log holds the duplicated derived index for `--derived`. That is the precondition for
/// separating "this prune did nothing to the other store" from "there was nothing to do".
fn seed_both_stores(root: &Path, rounds: u64) {
    let id = project_identity(root);
    let mut events = vec![
        Event::new("RunStarted", br#"{"run":"dead","criteria":["c"]}"#.to_vec())
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(10)),
    ];
    events.push(
        Event::new(
            "DecisionMade",
            format!(
                r#"{{"id":"{DEAD_DECISION}","summary":"s","governs":["src/a.rs"],"supersedes":""}}"#
            )
            .into_bytes(),
        )
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(11)),
    );
    events.push(
        Event::new("RunStarted", br#"{"run":"live","criteria":["c"]}"#.to_vec())
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(20)),
    );
    for r in 0..rounds {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            entity("alpha"),
            KEY_DEF,
            1_000 + r,
        ));
        events.push(keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            edge("alpha"),
            KEY_REF,
            1_000 + r,
        ));
    }

    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &id);
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed the event log");

    // Fold the log AS WRITTEN - each event carrying the position the store gave it - so the graph
    // is the projection of this log rather than of a pre-append copy of it.
    let written = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .expect("read the seeded log back");
    let graph =
        Projector::open(graph_db(root).to_str().unwrap(), &id).expect("open the context graph");
    graph.apply_batch(&written).expect("fold the seeded log");
}

/// `rigger reset` drives TWO prunes over TWO stores, and each one tells the operator it left the
/// other alone: `reset --runs` prints that it deletes no event from the log, and the shipped
/// `--derived` guidance says the live graph is unchanged. Both claims are load-bearing precisely BECAUSE the modes
/// compose - an operator who runs them together has no way to attribute a loss to one of them, so
/// each has to be safe for the other's store on its own.
///
/// Neither claim is reachable from a test that runs one mode against one store, which is why
/// nothing held them before: `tests/reset_derived_compaction.rs` never populates a context graph,
/// and the `--runs` prune predates this unit's second mode. Here both stores are populated with a
/// pile for EACH prune - a dead run's node for `--runs`, duplicated derived index rows for
/// `--derived` - so "it did not touch the other store" is separated from "there was nothing to do".
///
/// The DISSOCIATION is the point, in both directions: `--runs` changes the graph and leaves every
/// event row byte-for-byte, `--derived` deletes event rows and leaves the graph's content
/// identical. The composition then has exactly one honest outcome - the two effects, neither more
/// nor less - and that is asserted against the two single-mode results rather than restated, so a
/// composed reset that pruned harder (or that let one mode's read see the other's writes) cannot
/// pass by agreeing with a hand-written expectation.
#[test]
fn each_reset_mode_sheds_only_its_own_accumulation_and_composing_them_does_exactly_both() {
    const ROUNDS: u64 = 5;

    let runs_only = pinned_project();
    let derived_only = pinned_project();
    let composed = pinned_project();
    for project in [&runs_only, &derived_only, &composed] {
        assert_eq!(
            project_identity(project.path()),
            PINNED_ID,
            "the fixtures must all resolve to one identity, or their stores are not comparable"
        );
        seed_both_stores(project.path(), ROUNDS);
    }

    let seed_log = raw_rows(&event_log(runs_only.path()));
    let seed_graph = graph_rows(&graph_db(runs_only.path()));
    assert!(
        !seed_log.is_empty() && !seed_graph.0.is_empty() && !seed_graph.1.is_empty(),
        "the seed must populate BOTH stores, or nothing below proves anything"
    );
    assert!(
        shape(&raw_rows(&event_log(derived_only.path()))) == shape(&seed_log),
        "the three fixtures must start from an identical log; they differ at {}",
        first_difference(
            &row_marks(&raw_rows(&event_log(derived_only.path()))),
            &row_marks(&seed_log)
        )
    );
    assert_eq!(
        graph_rows(&graph_db(derived_only.path())),
        seed_graph,
        "the three fixtures must start from an identical graph"
    );

    // `--runs` PRUNES THE GRAPH AND ONLY THE GRAPH. Its own report promises it deletes no event,
    // so every row keeps its bytes AND its numbering: a row that survived but was renumbered or
    // repositioned is not an untouched log. This project is already under its minted identity, so
    // the identity migration `cmd_reset` runs first is a no-op here and the log really is
    // byte-for-byte identical; the one store class where it is NOT is pinned by
    // reset_runs_alone_migrates_a_legacy_store_and_its_report_says_what_that_wrote.
    let (out, err, ok) = run_rigger(runs_only.path(), &["reset", "--runs"]);
    assert!(ok, "reset --runs must succeed; stderr: {err}\n{out}");
    let after_runs_log = raw_rows(&event_log(runs_only.path()));
    assert!(
        after_runs_log == seed_log,
        "reset --runs reports that it deletes no event, so every row must survive \
         byte-for-byte, position and revision included; the log differs at {}, and the command \
         said: {out:?}",
        first_difference(&row_marks(&after_runs_log), &row_marks(&seed_log))
    );
    let after_runs_graph = graph_rows(&graph_db(runs_only.path()));
    let dropped: Vec<&String> = seed_graph
        .0
        .iter()
        .filter(|n| !after_runs_graph.0.contains(n))
        .collect();
    assert!(
        dropped.iter().any(|n| n.contains(DEAD_DECISION)),
        "the --runs prune must actually drop the dead run's decision node, or 'the log survived \
         it' is a claim about a prune that did nothing; it dropped {dropped:?}"
    );

    // `--derived` COMPACTS THE LOG AND ONLY THE LOG. The shipped guidance tells an operator the
    // live graph is unchanged - which it can be, because every recording of a key folds to the
    // same rows, so dropping the superseded ones changes nothing the projection holds.
    let (out, err, ok) = run_rigger(derived_only.path(), &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    assert_eq!(
        graph_rows(&graph_db(derived_only.path())),
        seed_graph,
        "the shipped guidance says the live graph is unchanged by --derived, so its live content \
         must be identical; the command said: {out:?}"
    );
    let after_derived_log = raw_rows(&event_log(derived_only.path()));
    assert_eq!(
        seed_log.len() - after_derived_log.len(),
        2 * (ROUNDS as usize - 1),
        "the --derived prune must shed the superseded recordings of both keys, or 'the graph \
         survived it' is a claim about a prune that did nothing; it said: {out:?}"
    );

    // COMPOSED: exactly the two effects. The log is what `--derived` alone leaves and the graph is
    // what `--runs` alone leaves - so neither mode prunes harder in company, and neither one's read
    // is disturbed by the other's writes.
    let (out, err, ok) = run_rigger(composed.path(), &["reset", "--runs", "--derived"]);
    assert!(
        ok,
        "reset --runs --derived must succeed; stderr: {err}\n{out}"
    );
    let composed_log = raw_rows(&event_log(composed.path()));
    assert!(
        shape(&composed_log) == shape(&after_derived_log),
        "the composed reset must leave exactly the log --derived alone leaves; it differs at {}, \
         and the command said: {out:?}",
        first_difference(&row_marks(&composed_log), &row_marks(&after_derived_log))
    );
    assert_eq!(
        graph_rows(&graph_db(composed.path())),
        after_runs_graph,
        "the composed reset must leave exactly the graph --runs alone leaves; it said: {out:?}"
    );
}

// ---------------------------------------------------------------------------------------
// 9. The run history an operator reads, after a compaction, through the binary
// ---------------------------------------------------------------------------------------

/// How many rows of a DERIVED index type the log holds - the pile the prune is entitled to shed,
/// counted apart from everything else it is not.
fn derived_rows(db: &Path) -> usize {
    raw_rows(db)
        .iter()
        .filter(|r| rigger::ingest::is_derived_index_type(&r.2))
        .count()
}

/// Seed a project with a real run history - two runs, one clean unit and one escalated, each
/// gated - INTERLEAVED with the derived duplication the prune exists to shed, so the compaction
/// deletes rows from the very stream the run read-model reads.
fn seed_run_history_and_duplication(root: &Path, rounds: u64) {
    let mut events: Vec<Event> = Vec::new();
    let mut at = 10u64;
    let push = |type_: &str, data: &str, at: &mut u64| {
        *at += 1;
        Event::new(type_, data.as_bytes().to_vec())
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(*at))
    };
    events.push(push(
        "RunStarted",
        r#"{"run":"r1","criteria":["c one"]}"#,
        &mut at,
    ));
    events.push(push(
        "UnitStarted",
        r#"{"id":"u1","agent":"worker"}"#,
        &mut at,
    ));
    events.push(push(
        "GateVerdict",
        r#"{"gate":"tests","pass":true}"#,
        &mut at,
    ));
    events.push(push(
        "UnitIntegrated",
        r#"{"id":"u1","commit":"aaa"}"#,
        &mut at,
    ));
    for r in 0..rounds {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            entity("alpha"),
            KEY_DEF,
            1_000 + r,
        ));
    }
    events.push(push(
        "RunStarted",
        r#"{"run":"r2","criteria":["c two"]}"#,
        &mut at,
    ));
    events.push(push(
        "UnitStarted",
        r#"{"id":"u2","agent":"worker"}"#,
        &mut at,
    ));
    events.push(push(
        "GateVerdict",
        r#"{"gate":"tests","pass":false}"#,
        &mut at,
    ));
    events.push(push("UnitEscalated", r#"{"id":"u2"}"#, &mut at));
    for r in 0..rounds {
        events.push(keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            edge("alpha"),
            KEY_REF,
            2_000 + r,
        ));
    }

    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &project_identity(root));
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed the run history");
}

/// The shipped guidance promises, in the paragraph that tells an operator it is safe to delete
/// from an append-only log, that "the whole run history `rigger stats` and replay read" survives
/// the compaction. Section 6 proves the SENTENCE ships; this proves the BINARY honors it.
///
/// Rows surviving in the table is not the same promise, for two reasons this test exists to close.
/// The run read-model rides the namespace-scoped GLOBAL read - a `LIKE`-filtered scan across the
/// whole file - not the `read_stream` the revision-cursor test drives, and the two prefix
/// comparisons in this store are deliberately not the same one. And the fold that turns those
/// events into a report is index-and-order sensitive: unit outcomes are attributed to the run
/// whose `RunStarted` precedes them, so deleting rows from the MIDDLE of the stream is exactly the
/// shape that would misattribute a unit to the wrong run while every surviving row still looked
/// perfectly intact in the table.
///
/// So the report itself is compared, byte for byte, across the compaction - in both views, since
/// the default view reads only the latest run while `--all` folds every run in the log. The
/// duplication is interleaved BETWEEN the two runs so the prune deletes from between them, and the
/// pruned count is asserted first: an equality across a prune that removed nothing proves nothing.
#[test]
fn the_run_history_the_shipped_guidance_promises_reads_back_identically_after_a_compaction() {
    const ROUNDS: u64 = 6;
    let dir = temp_project();
    let root = dir.path();
    seed_run_history_and_duplication(root, ROUNDS);

    let (before, err, ok) = run_rigger(root, &["stats"]);
    assert!(ok, "stats must succeed on the seeded log; stderr: {err}");
    let (before_all, err, ok) = run_rigger(root, &["stats", "--all"]);
    assert!(
        ok,
        "stats --all must succeed on the seeded log; stderr: {err}"
    );

    // The report must actually be a report of this history, or the equality below is an equality
    // between two "no runs" messages.
    assert!(
        before.contains("(1/1 units escalated"),
        "the default view must report the LATEST run's escalation; got:\n{before}"
    );
    assert!(
        before_all.contains("(1/2 units escalated"),
        "the --all view must aggregate both seeded runs; got:\n{before_all}"
    );

    let derived_before = derived_rows(&event_log(root));
    assert_eq!(
        derived_before,
        2 * ROUNDS as usize,
        "the seed must actually bloat the log with both keys' re-recordings"
    );
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    // The precondition is stated over the DERIVED rows alone, deliberately: a total-row delta
    // would also be satisfied by a prune that ate a run event for every duplicate it spared, which
    // is precisely the damage the equality below exists to catch.
    assert_eq!(
        derived_rows(&event_log(root)),
        2,
        "the compaction must leave one recording per key, or the report's survival is a claim \
         about a prune that did nothing; it said: {out:?}"
    );

    let (after, err, ok) = run_rigger(root, &["stats"]);
    assert!(
        ok,
        "stats must still succeed on the compacted log; stderr: {err}"
    );
    assert_eq!(
        after, before,
        "the shipped guidance promises the run history rigger stats reads survives the \
         compaction, so the report must be byte-identical across it"
    );

    let (after_all, err, ok) = run_rigger(root, &["stats", "--all"]);
    assert!(
        ok,
        "stats --all must still succeed on the compacted log; stderr: {err}"
    );
    assert_eq!(
        after_all, before_all,
        "the historical aggregate reads EVERY run in the log, so it must survive the compaction \
         whole - not just the latest run's slice of it"
    );
}

// ---------------------------------------------------------------------------------------
// 10. The stream boundary INSIDE one namespace: a replay key names a generation PER STREAM
// ---------------------------------------------------------------------------------------

/// Every recording of `key` the log holds in `stream`, in position order.
fn rows_of_key(rows: &[Row], stream: &str, key: &str) -> Vec<Row> {
    rows.iter()
        .filter(|r| r.1 == stream && replay_key(r).as_deref() == Some(key))
        .cloned()
        .collect()
}

/// The prune ranks a key's recordings WITHIN EACH STREAM, and every fixture above - here and in
/// the unit's own suite - seeds exactly one stream per namespace (`conductor::STREAM`), so the
/// `stream` half of that partition is asserted nowhere.
///
/// It is not decoration. A project namespace holds MORE than one stream under the one
/// `proj-<id>-` prefix (`conductor::STREAM` and `canary::STREAM` are both written through the same
/// decorator), and the prune chooses its candidates by TYPE and by PREFIX - never by stream - so
/// every stream in the namespace is eligible for it. Rank across the namespace instead of within
/// the stream and the older stream does not lose a duplicate, it loses its ONLY surviving row, to
/// a stream it has nothing to do with: silent deletion of the last recording of a live key, which
/// is the one outcome the fail-safe direction of this command forbids.
///
/// The same boundary, in the cursor the compaction moved: `Store::append` reads the stream's
/// current revision as `MAX(revision) WHERE stream = ?1`. Section 4 pins that on a single gapped
/// stream, where a per-stream cursor and a namespace-wide one cannot be told apart; two gapped
/// streams can tell them apart, so each is asked for its own.
#[test]
fn each_stream_in_one_namespace_keeps_its_own_latest_recording_and_answers_its_own_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("streams.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    const PROJECT: &str = "two-streams";
    const ROUNDS: u64 = 5;
    let streams = [rigger::conductor::STREAM, rigger::canary::STREAM];

    // ONE namespace, TWO streams, each re-recording the SAME replay key. The run stream is written
    // first, so ALL of its recordings sit at lower positions than any of the canary stream's: a
    // ranking that forgot the stream would rank every one of them below the other stream's latest
    // and delete the lot.
    {
        let store = Namespaced::new(&backend, PROJECT);
        for (s, stream) in streams.iter().enumerate() {
            let events: Vec<Event> = (0..ROUNDS)
                .map(|r| {
                    keyed(
                        rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        entity(stream),
                        KEY_DEF,
                        1_000 + s as u64 * 100 + r,
                    )
                })
                .collect();
            store
                .append(stream, ExpectedRevision::Any, &events)
                .expect("seed the stream");
        }
    }

    let before = raw_rows(&db);
    let scoped = |stream: &str| format!("{}{stream}", Namespaced::prefix_for(PROJECT));
    for stream in streams {
        assert_eq!(
            rows_of_key(&before, &scoped(stream), KEY_DEF).len(),
            ROUNDS as usize,
            "the seed must give {stream} its own pile of re-recordings, or the prune below has \
             nothing to choose between"
        );
    }

    let pruned = prune_all_types(&backend, &Namespaced::prefix_for(PROJECT));
    assert_eq!(
        pruned.total_removed(),
        2 * (ROUNDS as usize - 1),
        "each stream must shed its OWN superseded recordings - one survivor per stream, not one \
         survivor per namespace; got {:?}",
        pruned.removed
    );

    // EACH STREAM KEEPS ITS OWN LATEST, byte for byte, position and revision included.
    let after = raw_rows(&db);
    for stream in streams {
        let scoped = scoped(stream);
        let own_latest = rows_of_key(&before, &scoped, KEY_DEF)
            .into_iter()
            .max_by_key(|r| r.0)
            .expect("the seed recorded the key in this stream");
        assert_eq!(
            rows_of_key(&after, &scoped, KEY_DEF),
            vec![own_latest],
            "{stream} must keep the latest recording IT holds of the shared key, untouched"
        );
    }

    // AND EACH ANSWERS ITS OWN CURSOR. Both streams were gapped down to a single row at revision
    // ROUNDS-1, so both refuse a stale cursor by naming that revision, and both accept it.
    let store = Namespaced::new(&backend, PROJECT);
    let stale = ExpectedRevision::Exact(0);
    let cursor = ROUNDS as i64 - 1;
    for stream in streams {
        match store.append(stream, stale, &[Event::new("Stale", b"{}".to_vec())]) {
            Err(Error::Conflict { actual, .. }) => assert_eq!(
                actual, cursor,
                "{stream} must report ITS OWN highest surviving revision on a conflict"
            ),
            other => panic!(
                "a stale cursor must conflict on the compacted stream {stream}; got {other:?}"
            ),
        }
        store
            .append(
                stream,
                ExpectedRevision::Exact(cursor),
                &[Event::new("RunStarted", br#"{"run":"r1"}"#.to_vec())],
            )
            .unwrap_or_else(|e| panic!("{stream} must accept its own cursor {cursor}; got {e:?}"));
    }
    for stream in streams {
        let log = store
            .read_stream(stream, 0, Direction::Forward)
            .expect("read the compacted stream back");
        assert_eq!(
            log.iter().map(|e| e.revision).collect::<Vec<_>>(),
            vec![cursor, cursor + 1],
            "{stream} must read back its own survivor and its own appended event"
        );
        assert_eq!(
            log.last().map(|e| e.type_.as_str()),
            Some("RunStarted"),
            "the event appended to {stream} must land in {stream}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// 11. The OTHER command the criterion names: `rigger status` over a compacted log
// ---------------------------------------------------------------------------------------

/// Append `events` to the project's run stream, timestamped in order after `at`.
fn append_run(root: &Path, at: &mut u64, events: &[(&str, &str)]) {
    let staged: Vec<Event> = events
        .iter()
        .map(|(type_, data)| {
            *at += 1;
            Event::new(*type_, data.as_bytes().to_vec())
                .with_valid_from(UNIX_EPOCH + Duration::from_secs(*at))
        })
        .collect();
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    Namespaced::new(&backend, &project_identity(root))
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &staged)
        .expect("seed the run stream");
}

/// Append one pile of re-recordings of `key` - the duplication the prune sheds - into the run
/// stream, so the prune deletes from wherever in the run's own slice this pile was placed.
fn append_duplication(root: &Path, key: &str, rounds: u64, base_secs: u64) {
    let events: Vec<Event> = (0..rounds)
        .map(|r| {
            keyed(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                entity("alpha"),
                key,
                base_secs + r,
            )
        })
        .collect();
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    Namespaced::new(&backend, &project_identity(root))
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed the duplication");
}

/// Criterion 5 names TWO commands that must read the compacted store correctly: `rigger validate`
/// and `rigger status`. The unit's own suite drives `validate`; nothing drives `status`.
///
/// It is not covered by the `stats` test above either, because it is not the same read. `stats`
/// rides the namespace-scoped GLOBAL read; `status` reads the run stream with `read_stream`, cuts
/// it to the CURRENT run at the last `RunStarted`, and hands that one slice to four separate
/// order-sensitive folds: the in-flight view, the replay driver's parked wave, the blocker
/// classifier (whose "attempt n" is the attempt count carried by the LAST recorded failure, plus
/// one), and the release-ready projection. Deleting rows out of the middle of that slice is
/// exactly the shape that moves a boundary or a count while every surviving row still looks
/// perfectly intact, and it is the surface an operator watches a run through.
///
/// Both shapes an operator sees are pinned, in sequence, over ONE project: a run BLOCKED
/// mid-remediation (the blocker line and its attempt number) and then the same run DONE (the
/// release-ready handoff). The second compaction therefore also runs against an already-compacted
/// log, which is the state a real project is in the second time an operator prunes it.
///
/// The `--json` view is deliberately not compared: it carries only the spawns parked in flight,
/// which a seeded log has none of, so an equality there would be an equality between two empty
/// arrays. The rendered view is the one that folds the run.
#[test]
fn the_status_view_reads_a_compacted_log_exactly_as_it_read_the_bloated_one() {
    const ROUNDS: u64 = 6;
    let dir = temp_project();
    let root = dir.path();

    // A finished earlier run, then the duplication, then the current run - which is blocked
    // mid-remediation, with its own pile of duplication INSIDE its slice, between the failure the
    // attempt count is derived from and the retry that reads it.
    let mut at = 10u64;
    append_run(
        root,
        &mut at,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["c one"]}"#),
            (rigger::ledger::TYPE_UNIT_STARTED, r#"{"id":"u1"}"#),
            (
                rigger::ledger::TYPE_UNIT_INTEGRATED,
                r#"{"id":"u1","commit":"aaa"}"#,
            ),
        ],
    );
    append_duplication(root, KEY_DEF, ROUNDS, 1_000);
    append_run(
        root,
        &mut at,
        &[
            ("RunStarted", r#"{"run":"r2","criteria":["c two"]}"#),
            (rigger::ledger::TYPE_UNIT_STARTED, r#"{"id":"u2"}"#),
            (
                rigger::ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u2","attempts":2}"#,
            ),
        ],
    );
    append_duplication(root, KEY_REF, ROUNDS, 2_000);
    append_run(
        root,
        &mut at,
        &[(rigger::ledger::TYPE_UNIT_STARTED, r#"{"id":"u2"}"#)],
    );

    let status = |label: &str| {
        let (out, err, ok) = run_rigger(root, &["status"]);
        assert!(
            ok,
            "rigger status must succeed {label} the compaction; stderr: {err}\n{out}"
        );
        out
    };

    // The report must be a report of THIS run, or the equalities below are equalities between two
    // "no run" messages.
    let blocked_before = status("before");
    assert!(
        blocked_before.contains("run r2"),
        "status must read the CURRENT run out of the seeded log; got:\n{blocked_before}"
    );
    assert!(
        blocked_before.contains("u2: building (attempt 3)"),
        "status must classify the blocked unit and count its attempt off the recorded failure; \
         got:\n{blocked_before}"
    );

    let compact = |label: &str, expect_removed: usize| {
        let derived_before = derived_rows(&event_log(root));
        let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
        assert!(
            ok,
            "reset --derived must succeed {label}; stderr: {err}\n{out}"
        );
        let derived_after = derived_rows(&event_log(root));
        assert_eq!(
            derived_before - derived_after,
            expect_removed,
            "the compaction {label} must actually delete from the run's stream, or the equality \
             it is checked by proves nothing; it said: {out:?}"
        );
    };

    // The prune deletes from BOTH sides of the current run's boundary: the pile before its
    // `RunStarted` and the pile inside its slice.
    compact("on the bloated log", 2 * (ROUNDS as usize - 1));
    assert_eq!(
        status("after"),
        blocked_before,
        "the blocked run's status view must read back identically across a compaction that \
         deleted from the middle of the very slice it folds"
    );

    // The same surface in the run's other shape: integrate the unit, bloat the log again, and
    // compact a log that has ALREADY been compacted once.
    append_run(
        root,
        &mut at,
        &[(
            rigger::ledger::TYPE_UNIT_INTEGRATED,
            r#"{"id":"u2","commit":"bbb"}"#,
        )],
    );
    append_duplication(root, KEY_DEF, ROUNDS, 3_000);

    let done_before = status("before the second compaction");
    assert!(
        done_before.contains("release-ready:") && done_before.contains("1 unit integrated"),
        "a done run must surface the release-ready handoff, or the equality below is checking a \
         blank; got:\n{done_before}"
    );
    // ROUNDS, not ROUNDS-1: the new pile re-records a key whose survivor the FIRST compaction
    // left behind, so this prune sheds that stale survivor too and one recording again remains.
    compact("on the already-compacted log", ROUNDS as usize);
    assert_eq!(
        status("after the second compaction"),
        done_before,
        "the done run's release-ready handoff must survive a second compaction whole"
    );
}

// ---------------------------------------------------------------------------------------
// 12. The rows the prune shares with the storage guard
// ---------------------------------------------------------------------------------------

/// Split a `<prefix>/<file>@<hash>#<i>` content key into `(the prefix every key naming the same
/// file begins with, the content generation this key belongs to)` - the shape the ingest layer
/// mints.
///
/// It is written HERE, in the test, because the split is INJECTED configuration: the store parses
/// no key format of its own, so a caller that wants the guard hands it this policy. Splitting from
/// the RIGHT is load-bearing - a real path may itself contain `@` or `#`.
fn path_subject_of(key: &str) -> Option<(Range<usize>, Range<usize>)> {
    let (prefix, rest) = key.split_once('/')?;
    if prefix.is_empty() || rest.is_empty() {
        return None;
    }
    let (head, index) = key.rsplit_once('#')?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (file, hash) = head.rsplit_once('@')?;
    if file.len() <= prefix.len() + 1 || hash.is_empty() {
        return None;
    }
    let subject_end = file.len() + 1; // through the `@` that ends the subject
    Some((0..subject_end, subject_end..subject_end + hash.len()))
}

/// The guard policy this project would configure: its real metadata key, its real derived index
/// types, and the split for the keys it really mints - so the guard is exercised against the same
/// vocabulary the prune is handed, which is the whole point of asking whether they agree.
fn guard_policy() -> ContentIdentity {
    ContentIdentity::new(
        rigger::ingest::META_REPLAY_KEY,
        rigger::ingest::DERIVED_INDEX_TYPES,
        path_subject_of,
    )
}

/// The two events one content generation of `gc/src/guarded.rs` records: the entity at `#0` and
/// the edge at `#1`, exactly as a keyed ingest batch shapes them.
fn guarded_generation(hash: &str, secs: u64) -> Vec<Event> {
    vec![
        keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            entity(hash),
            &format!("gc/src/guarded.rs@{hash}#0"),
            secs,
        ),
        keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            edge(hash),
            &format!("gc/src/guarded.rs@{hash}#1"),
            secs,
        ),
    ]
}

/// Every `(replay key, position)` the log holds, in position order - the raw material BOTH layers
/// read: the prune ranks these to choose what to delete, and the guard's latest-generation walk
/// reads the greatest position per key to decide which generation a subject is at.
fn keyed_positions(db: &Path) -> Vec<(String, i64)> {
    raw_rows(db)
        .iter()
        .filter_map(|row| replay_key(row).map(|k| (k, row.0)))
        .collect()
}

/// The guard's verdicts on a store, as an outside caller sees them: for each generation probed, one
/// `true` per event the store SUPPRESSED and one `false` per event it wrote.
///
/// The probe is a re-ingest of two generations in a fixed order - first the one the subject is
/// currently at, then one it has moved past - which is precisely the pair the latest-per-subject
/// rule has to tell apart. It runs on a FRESH handle, because a compaction is something an operator
/// does between processes: the answer a long-lived writer had cached is not the answer that matters.
fn guard_verdicts(db: &Path, project: &str, hashes: [&str; 2]) -> Vec<Vec<bool>> {
    let backend = Store::open(db.to_str().unwrap())
        .expect("open the compacted log")
        .with_content_identity(guard_policy());
    let store = Namespaced::new(&backend, project);
    hashes
        .iter()
        .enumerate()
        .map(|(i, hash)| {
            let appended = store
                .append(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &guarded_generation(hash, 9_000 + i as u64),
                )
                .expect("the guarded re-ingest is accepted");
            appended.placements().iter().map(Option::is_none).collect()
        })
        .collect()
}

/// The compaction and the storage guard read the SAME rows, and the prune must leave every one of
/// the guard's verdicts exactly where it found it.
///
/// The two features meet on one fact: which recording of a covered replay key is the LATEST one in
/// its stream. The prune keeps that row and deletes the rest; the guard reads the greatest position
/// per key to decide which generation a subject is currently at, and suppresses only an append of
/// THAT generation. Keeping any other recording of a key would still leave one row per key - a log
/// that looks perfectly compacted, whose every assertion about survivors, sizes and folds still
/// holds - while silently moving the subject's current generation, so the store would afterwards
/// swallow a re-ingest of the content the tree really holds and admit one it has moved past. That
/// is the graph-on-a-superseded-version outcome the spec forbids, reached through the compaction
/// rather than through the dedup.
///
/// Neither layer's own suite can see it: the guard's periphery suite never prunes, and this
/// criterion's suites never configure a guard. So the property is asserted DIFFERENTIALLY, over two
/// logs seeded identically, one compacted and one not:
///
///   - the fixture makes the answer non-obvious on purpose. The subject records generation `h1`,
///     then `h2`, then `h1` AGAIN - a revert - so its current generation is neither the
///     first-recorded nor the last-minted one, and a prune that kept the earliest recording of each
///     key rather than the latest would flip it;
///   - the un-compacted log's verdicts are asserted ABSOLUTELY first (suppress the current
///     generation, write the superseded one), so the equality that follows is an equality between
///     two known-meaningful answers rather than between two coincidences;
///   - and the compacted log's own appends are checked to carry no degradation marker, because a
///     guard that stopped judging suppresses nothing and would answer `false` everywhere for a
///     reason that has nothing to do with the prune.
#[test]
fn a_compaction_leaves_the_storage_guards_verdicts_exactly_where_it_found_them() {
    const PROJECT: &str = "guarded";
    // h1, then h2, then back to h1: the subject's CURRENT generation is h1, and it is neither the
    // first thing recorded nor the last generation minted.
    const HISTORY: [&str; 8] = ["h1", "h1", "h1", "h2", "h2", "h2", "h1", "h1"];

    let dir = tempfile::tempdir().unwrap();
    let bloated = dir.path().join("bloated.db");
    let compacted = dir.path().join("compacted.db");

    // Seeded through an UNGUARDED handle, which is the log this command exists for: duplication a
    // store accreted before anything suppressed it.
    for db in [&bloated, &compacted] {
        let backend = Store::open(db.to_str().unwrap()).expect("open a fresh log");
        let store = Namespaced::new(&backend, PROJECT);
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[
                    Event::new("RunStarted", br#"{"run":"g","criteria":["c"]}"#.to_vec())
                        .with_valid_from(UNIX_EPOCH + Duration::from_secs(10)),
                ],
            )
            .expect("seed the non-derived event");
        for (i, hash) in HISTORY.iter().enumerate() {
            store
                .append(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &guarded_generation(hash, 1_000 + i as u64),
                )
                .expect("seed a generation");
        }
    }
    assert_eq!(
        keyed_positions(&bloated),
        keyed_positions(&compacted),
        "the two logs must be seeded identically, or the differential below compares two fixtures \
         rather than one compaction"
    );

    // Compact ONE of them, through the primitive `rigger reset --derived` drives.
    let bloated_positions = keyed_positions(&bloated);
    let pruned = {
        let backend = Store::open(compacted.to_str().unwrap()).expect("open the log to compact");
        prune_all_types(&backend, &Namespaced::prefix_for(PROJECT))
    };
    assert_eq!(
        pruned.total_removed(),
        12,
        "four distinct keys recorded 16 times must lose 12 recordings; got {pruned:?}"
    );

    // What survived is each key's LATEST recording - the row the guard's walk reads - and every
    // key still has exactly one. This is the mechanism the verdict equality below rests on, so it
    // is asserted directly rather than inferred from the fact that the counts came out right.
    let mut latest: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for (key, at) in &bloated_positions {
        let slot = latest.entry(key.clone()).or_insert(*at);
        *slot = (*slot).max(*at);
    }
    assert_eq!(
        keyed_positions(&compacted),
        {
            let mut survivors: Vec<(String, i64)> =
                latest.iter().map(|(k, at)| (k.clone(), *at)).collect();
            survivors.sort_by_key(|(_, at)| *at);
            survivors
        },
        "the compacted log must hold exactly the LATEST recording of every key, at its original \
         position - that row IS the guard's answer to which generation the subject is at"
    );

    // The verdicts themselves. First absolutely, on the log nobody compacted: the current
    // generation is suppressed, the superseded one is written.
    let before = guard_verdicts(&bloated, PROJECT, ["h1", "h2"]);
    assert_eq!(
        before,
        vec![vec![true, true], vec![false, false]],
        "the guard must suppress a re-ingest of the generation the subject is CURRENTLY at (h1, \
         reverted to) and write one it has moved past (h2), or this test is comparing two \
         meaningless answers"
    );

    // Then the same probe on the compacted log: identical, event for event.
    assert_eq!(
        guard_verdicts(&compacted, PROJECT, ["h1", "h2"]),
        before,
        "a compaction must leave every one of the guard's verdicts where it found them: the prune \
         deletes the very rows the latest-generation walk reads, so keeping the wrong recording of \
         a key would silently move the subject's current generation"
    );

    // And the guard was JUDGING while it answered, not silently switched off by a log that had
    // just been rewritten and vacuumed under it.
    assert!(
        raw_rows(&compacted)
            .iter()
            .all(|row| !row.5.contains(META_GUARD_DEGRADED)),
        "no event on the compacted log may carry a degradation marker - a guard that cannot probe \
         suppresses nothing, which would make the equality above hold for the wrong reason"
    );
}

// ---------------------------------------------------------------------------------------
// 13. The compare-and-append that rides ABOVE the gaps
// ---------------------------------------------------------------------------------------

/// The parked spawn the fixture below records, and the id the courier answers it with.
const COURIER_ID: &str = "u-compaction/implementer#1";

/// Run `rigger <args...>` in `cwd` under a WALL-CLOCK BOUND: `Some((stdout, stderr, success))`
/// when the binary exited on its own, `None` when the bound expired first (the child is killed
/// and reaped either way, so a bound that expires leaks no process).
///
/// The bound is an ASSERTION, not a convenience. The write this section exercises is a retry
/// loop, and its failure mode on a stream it cannot address is not a failed command - it is a
/// command that never returns, re-reading and re-deciding forever. A plain blocking run would
/// hang the whole test binary instead of failing it, which is the one outcome that would hide
/// this regression rather than report it, so the child is driven as a subprocess under a deadline
/// and a bound that expires is the failure.
///
/// Both pipes are drained only after exit. The command's output is a handful of lines, far below
/// the operating system's pipe buffer, so it cannot fill one and block on a write while this
/// thread waits for it.
fn run_rigger_bounded(
    cwd: &Path,
    args: &[&str],
    bound: Duration,
) -> Option<(String, String, bool)> {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    let mut child = Command::new(common::rigger_bin())
        .args(args)
        .current_dir(cwd)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn the rigger binary");
    let deadline = Instant::now() + bound;
    loop {
        match child.try_wait().expect("wait on the rigger binary") {
            Some(_) => {
                let out = child
                    .wait_with_output()
                    .expect("collect the binary's output");
                return Some((
                    String::from_utf8_lossy(&out.stdout).into_owned(),
                    String::from_utf8_lossy(&out.stderr).into_owned(),
                    out.status.success(),
                ));
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Seed `root`'s run stream with the shape a compaction gaps AROUND a parked spawn: a run start
/// and the SPAWN REQUEST the courier will answer, then `rounds` re-recordings of two derived
/// keys.
///
/// The two non-derived events sit at the HEAD of the stream, so the prune deletes strictly
/// between them and the surviving tail - the arrangement that pushes the stream's row count and
/// its revision cursor furthest apart while leaving a real, answerable spawn behind.
fn seed_run_with_a_parked_spawn(root: &Path, rounds: u64) {
    let backend = Store::open(event_log(root).to_str().unwrap()).unwrap();
    let project = project_identity(root);
    let store = Namespaced::new(&backend, &project);
    let mut events = vec![
        Event::new(
            "RunStarted",
            format!(r#"{{"run":"{project}","criteria":["c"]}}"#).into_bytes(),
        )
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(10)),
        Event::new(
            rigger::spawn::TYPE_SPAWN_REQUESTED,
            format!(
                r#"{{"id":"{COURIER_ID}","unit":"u-compaction","stage":"u-compaction","prompt":"build it"}}"#
            )
            .into_bytes(),
        )
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(11)),
    ];
    for r in 0..rounds {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            entity("alpha"),
            KEY_DEF,
            1_000 + r,
        ));
        events.push(keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            edge("alpha"),
            KEY_REF,
            1_000 + r,
        ));
    }
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed the run stream");
}

/// The derived index shares `conductor::STREAM` with the run's own events, so the compaction is
/// the FIRST and ONLY thing in this project that deletes rows from a stream an operator keeps
/// writing to. `prune_derived_index` accounts for the holes it leaves against exactly one
/// consumer - the sqlite `append`, which is why that cursor is now `MAX(revision)` and not
/// `COUNT(*) - 1` - and that accounting stops at the store boundary.
///
/// ONE caller lives above that boundary and supplies an [`ExpectedRevision::Exact`] of its own:
/// the compare-and-append behind `rigger result --if-absent`, this project's only optimistic
/// concurrency user. It reads the run stream, and if the spawn is still unanswered appends its
/// result pinned to the revision it just read. That is the write a courier makes on behalf of an
/// agent that died without self-reporting, and it is the only thing that can move a run past a
/// parked spawn.
///
/// Its expectation must come from the HEAD EVENT'S OWN REVISION, never from how many events the
/// read returned - the very distinction the compaction just forced one layer down. On a compacted
/// stream the two numbers are far apart, and the consequence of confusing them is not a failed
/// write that an operator would see: the loop treats a conflict as "the stream moved under me",
/// re-reads, computes the same unreachable expectation, and spins forever. The result is never
/// recorded, the spawn stays parked, and the run cannot advance - with the command still running.
///
/// Neither layer's own tests can see this. The compaction suites never record a result, and every
/// test of the courier's write runs on a densely numbered stream, where a count-derived cursor and
/// the real one agree and the bug is invisible.
#[test]
fn a_compacted_run_stream_still_answers_the_couriers_compare_and_append() {
    let dir = temp_project();
    let root = dir.path();
    const ROUNDS: u64 = 5;
    seed_run_with_a_parked_spawn(root, ROUNDS);

    let db = event_log(root);
    let head_before = raw_rows(&db)
        .iter()
        .map(|r| r.6)
        .max()
        .expect("the seed must populate the run stream");

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");

    // The compacted stream: the two head events, plus the latest recording of each of the two
    // keys. The tail is never eligible for deletion - a row is only pruned when a LATER recording
    // of its key exists - so the cursor stands exactly where it stood before the prune, while the
    // row count has collapsed to four.
    let compacted = raw_rows(&db);
    let head = compacted
        .iter()
        .map(|r| r.6)
        .max()
        .expect("a compacted stream must keep rows");
    assert_eq!(
        compacted.len(),
        4,
        "the prune must leave the two non-derived events and one recording of each key; got {:?}",
        compacted.iter().map(|r| r.2.clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        head, head_before,
        "the compaction must never delete a stream's head, so its revision cursor is exactly \
         where it was before the prune"
    );
    assert_ne!(
        head,
        compacted.len() as i64 - 1,
        "the fixture must actually separate the stream's row count from its revision cursor, or \
         the write below proves nothing about which of the two it used"
    );

    // THE COURIER'S WRITE. A bound, because the regression this guards does not return.
    let bound = Duration::from_secs(60);
    let Some((rout, rerr, rok)) = run_rigger_bounded(
        root,
        &["result", COURIER_ID, "the unit is green", "--if-absent"],
        bound,
    ) else {
        panic!(
            "`rigger result --if-absent` never returned within {bound:?} on a compacted stream: \
             the compare-and-append is pinning an expectation the stream cannot answer and is \
             retrying it forever, so the parked spawn can never be answered"
        )
    };
    assert!(
        rok,
        "the courier's write must land on a compacted stream; stdout: {rout}\nstderr: {rerr}"
    );

    // It landed ONCE, and it landed ABOVE the gaps: the next revision after the head the stream
    // actually holds, not after the count of rows that survived.
    let recorded = raw_rows(&db);
    let results: Vec<&Row> = recorded
        .iter()
        .filter(|r| r.2 == rigger::spawn::TYPE_SPAWN_RESULT)
        .collect();
    assert_eq!(
        results.len(),
        1,
        "the courier's result must be recorded exactly once; got {:?}",
        results.iter().map(|r| (r.0, r.6)).collect::<Vec<_>>()
    );
    assert_eq!(
        results[0].6,
        head + 1,
        "the recorded result must take the revision AFTER the stream's real head, so the write \
         was placed by the cursor the store keeps and not by the number of rows that survived"
    );

    // And it was purely ADDITIVE. Every row the compaction left keeps its position, its revision
    // and its bytes: answering a parked spawn on a compacted stream renumbers nothing.
    assert_eq!(
        recorded[..compacted.len()],
        compacted[..],
        "the courier's write must leave every surviving row exactly where the compaction left it"
    );

    // THE IDEMPOTENCE THE COURIER EXISTS FOR, on the same compacted stream. `--if-absent` is what
    // lets a death guard run unconditionally: it must read the gapped stream, SEE the result that
    // already stands, and write nothing - never a second, contradicting record.
    let Some((rout2, rerr2, rok2)) = run_rigger_bounded(
        root,
        &["result", COURIER_ID, "a second courier", "--if-absent"],
        bound,
    ) else {
        panic!(
            "a second `rigger result --if-absent` never returned within {bound:?} on a compacted \
             stream: the no-op path must decide from the rows it read, not retry"
        )
    };
    assert!(
        rok2,
        "a second --if-absent must succeed as a no-op; stdout: {rout2}\nstderr: {rerr2}"
    );
    assert!(
        rout2.contains("already has a result"),
        "the second --if-absent must report that it left the existing result untouched; got \
         {rout2:?}"
    );
    let after_second = raw_rows(&db);
    assert_eq!(
        after_second
            .iter()
            .filter(|r| r.2 == rigger::spawn::TYPE_SPAWN_RESULT)
            .count(),
        1,
        "a second --if-absent must leave the recorded result alone, not append a contradicting one"
    );
    assert_eq!(
        after_second, recorded,
        "the no-op must be a no-op on the log too, byte-for-byte"
    );
}

// ---------------------------------------------------------------------------------------
// 14. The OTHER reader the shipped sentence names: `rigger replay`, over a compacted log
// ---------------------------------------------------------------------------------------

/// Run `git <args...>` inside the throwaway repo, and fail the test with git's own message if it
/// does not succeed - a silently skipped `commit` would leave `--against HEAD` with no rev to
/// check out, and the test would then be asserting on an error message.
fn git_in(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("run git in the throwaway repo");
    assert!(
        out.status.success(),
        "git {args:?} must succeed in the throwaway repo: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
}

/// A throwaway project whose COMMITTED tree carries a valid rigger config.
///
/// `rigger replay --against <rev>` reads the candidate config OUT OF THE REPO at that rev, so a
/// project with an on-disk config but nothing committed cannot be replayed at all. `rigger init`
/// scaffolds that config and pins the project identity in `.rigger/project.id`, so it runs BEFORE
/// any seed: a seed written under the identity the directory name implies would land in a stream
/// the binary then never reads back.
fn committed_config_project() -> tempfile::TempDir {
    let dir = temp_project();
    let root = dir.path();
    // The repo's own identity, so the commit below never depends on the machine's git config.
    git_in(root, &["config", "user.email", "periphery@example.invalid"]);
    git_in(root, &["config", "user.name", "periphery"]);
    let (out, err, ok) = run_rigger(root, &["init"]);
    assert!(
        ok,
        "rigger init must scaffold the config `--against` checks out; stderr: {err}\n{out}"
    );
    git_in(root, &["add", "-A"]);
    git_in(root, &["commit", "-q", "-m", "the candidate config"]);
    dir
}

/// The shipped paragraph this unit adds tells an operator that deleting from an append-only log is
/// safe because "the whole run history `rigger stats` and replay read" survives it. Section 7
/// proves the SENTENCE ships and section 9 proves the binary honors the `stats` half of it. This
/// proves the half the sentence names second, which no test in either suite drives.
///
/// It is a different path through the store, not a second helping of section 9. `rigger stats`
/// folds the namespace-scoped GLOBAL read - a `LIKE`-filtered scan ordered by POSITION - and
/// `rigger status` (section 11) folds the current-run slice. `rigger replay` lifts its baseline
/// with `read_stream(conductor::STREAM, 0, Direction::Forward)`: the per-stream read, ordered by
/// the very REVISION sequence this compaction punches holes in, and the only one of the three
/// whose ordering key the prune touches. It then attributes that slice POSITIONALLY, folding unit
/// outcomes onto the `RunStarted` that precedes them, and the duplication the seed interleaves
/// sits BETWEEN the two seeded runs - exactly where a misordered or short read would move a unit
/// from one run's column into the other's while every surviving row still looked intact.
///
/// The equality is asserted on the printed diff, which is where an operator would see it, and it
/// is bracketed by two preconditions so it cannot pass vacuously: the command is shown to be
/// deterministic on this fixture BEFORE the prune (otherwise a difference afterwards would be
/// unattributable), and the prune is shown to have actually deleted rows from the stream the
/// baseline is read from (otherwise the equality is an equality across a no-op).
#[test]
fn the_replay_the_shipped_guidance_names_lifts_the_same_baseline_out_of_a_compacted_log() {
    const ROUNDS: u64 = 6;
    let dir = committed_config_project();
    let root = dir.path();
    seed_run_history_and_duplication(root, ROUNDS);

    let replay = ["replay", "latest", "--against", "HEAD"];
    let (before, err, ok) = run_rigger(root, &replay);
    assert!(
        ok,
        "replay must succeed against the seeded log; stderr: {err}\nstdout: {before}"
    );

    // The diff must be a diff OF THIS HISTORY. `latest` resolves to the second seeded run, and
    // its baseline column must carry that run's recorded outcome - without this, the equality
    // below could hold between two reports of an empty baseline.
    assert!(
        before.contains("baseline run r2"),
        "the replay must name the latest seeded run as its baseline; got:\n{before}"
    );
    // And the column must be the fold of THAT run, not of the first seeded one. The two runs are
    // deliberately mirror images - r1 integrates its unit on a passing gate, r2 escalates its unit
    // on a failing one - so an off-by-one slice of the stream would land on a baseline whose every
    // row still parses. `escalation rate` is the discriminator: 100% is r2, 0.0% is r1.
    let baseline_column = replay_baseline_column(&before);
    for (metric, expected) in [("units started", "1"), ("escalation rate", "100.0%")] {
        let found = baseline_column
            .iter()
            .find(|(m, _)| m == metric)
            .map(|(_, v)| v.as_str());
        assert_eq!(
            found,
            Some(expected),
            "the baseline column must be the fold of the LATEST seeded run: {metric} must read \
             {expected}; got:\n{before}"
        );
    }

    // PRECONDITION: the command is deterministic on this fixture. Asserted before anything is
    // pruned, so a difference after the prune can only be the prune.
    let (again, err, ok) = run_rigger(root, &replay);
    assert!(
        ok,
        "a second replay must succeed against the same log; stderr: {err}\nstdout: {again}"
    );
    assert_eq!(
        again, before,
        "replay must be deterministic on an unchanged log, or the comparison across the \
         compaction proves nothing about the compaction"
    );

    // PRECONDITION: the prune actually deletes from the stream the baseline is read from.
    let derived_before = derived_rows(&event_log(root));
    assert_eq!(
        derived_before,
        2 * ROUNDS as usize,
        "the seed must actually bloat the log with both keys' re-recordings"
    );
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    assert_eq!(
        derived_rows(&event_log(root)),
        2,
        "the compaction must leave one recording per key, or the baseline's survival is a claim \
         about a prune that did nothing; it said: {out:?}"
    );

    let (after, err, ok) = run_rigger(root, &replay);
    assert!(
        ok,
        "replay must still succeed against the compacted log; stderr: {err}\nstdout: {after}"
    );
    assert_eq!(
        after, before,
        "the shipped guidance promises the run history replay reads survives the compaction, so \
         the baseline it lifts through the gapped per-stream read must print identically"
    );
}

/// The `(metric, baseline)` pairs out of the replay diff's baseline column - the middle column of
/// `  <metric>  <baseline>  <candidate>`, under the two header lines.
///
/// Parsed from the RIGHT: a metric label contains spaces ("units started"), so splitting from the
/// left would read the label's own second word as the baseline value and every assertion built on
/// it would be meaningless. A changed row carries a trailing `*`, which is dropped first.
fn replay_baseline_column(out: &str) -> Vec<(String, String)> {
    out.lines()
        .skip_while(|l| !l.trim_start().starts_with("metric "))
        .skip(1)
        .filter_map(|l| {
            let mut cols: Vec<&str> = l.split_whitespace().collect();
            if cols.last() == Some(&"*") {
                cols.pop();
            }
            let _candidate = cols.pop()?;
            let baseline = cols.pop()?;
            (!cols.is_empty()).then(|| (cols.join(" "), baseline.to_string()))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// 15. The FOLD the maintenance now speaks for.
//
// The prune no longer only deletes rows: it CARRIES a pruned key's earliest valid-time onto the
// recording it keeps, for the types named by `ingest::reasserted_derived_types`, which is derived
// from the single fold fact `contextgraph::refold_supersedes_prior_edges`. That partition is a
// claim ABOUT THE FOLD made in a module the fold cannot see, and it is exactly the kind of claim
// that goes stale in silence: nothing about `matches!(type_, A | B)` fails when a fifth type is
// added, or when a fold arm changes which valid-time its live edge ends up holding. The store
// under it is equally blind - it takes the partition as data and would faithfully carry a date
// onto a type whose fold supersedes, MOVING the live graph while every surviving row still looked
// intact.
//
// So the partition is asserted against the fold ITSELF, type by type and empirically: for each
// derived type, a log carrying two recordings of one subject is pruned by the SHIPPED policy and
// re-folded, and the live graph - `valid_from` and provenance included - must be the graph the
// WHOLE log folds to. Then the same log is pruned by the INVERTED policy, which must move the
// graph: that half is what stops the equality above from passing for a type whose date nothing
// can shift.
// ---------------------------------------------------------------------------------------

/// The payload one recording of `type_` carries. All four name the SAME subject (`src/a.rs` and
/// the design doc that specifies it), so re-recording a type is a re-recording of one fact rather
/// than a new one - which is the shape the prune's per-key rule is about.
///
/// The code half sets `fresh`, because that is what a real extraction BATCH carries: the marker is
/// what makes the code fold supersede the file's prior edges, and a fixture without it would
/// exercise a fold arm no extraction pass ever drives.
fn derived_payload(type_: &str) -> Vec<u8> {
    let v = match type_ {
        t if t == rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED => serde_json::json!({
            "file": "src/a.rs", "name": "alpha", "kind": "function", "line": 1, "lang": "rust",
            "fresh": true,
        }),
        t if t == rigger::contextgraph::TYPE_EDGE_INFERRED => serde_json::json!({
            "file": "src/a.rs", "name": "beta", "lang": "rust", "fresh": true,
        }),
        t if t == rigger::contextgraph::TYPE_DOC_CONCEPT_EXTRACTED => serde_json::json!({
            "kind": rigger::contextgraph::KIND_DESIGN_DOC,
            "id": "docs/design.md",
            "title": "the design that specifies src/a.rs",
            "doc": "docs/design.md",
        }),
        t if t == rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED => serde_json::json!({
            "from": "docs/design.md", "to": "src/a.rs",
            "rel": rigger::contextgraph::REL_SPECIFIES,
        }),
        other => panic!(
            "the derived index gained the type {other:?} and this fixture does not know how to \
             record it - a new derived type must be placed by the fold before the prune can carry \
             or leave its dates"
        ),
    };
    serde_json::to_vec(&v).unwrap()
}

/// One replay key per type, in the shape the key authority builds
/// (`<prefix>/<file>@<hash>#<i>`). Distinct per type so a fixture never leans on the prune's
/// type-scoping to keep two subjects apart.
fn derived_key_for(type_: &str) -> String {
    format!("gc/src/{type_}.rs@h1#0")
}

/// Seed `project`'s run stream inside `backend` with one recording of `type_` per entry in
/// `dates`, all of them re-recordings of the SAME subject under the SAME replay key.
fn seed_one_derived_type(backend: &Store, project: &str, type_: &str, dates: &[u64]) {
    let key = derived_key_for(type_);
    let events: Vec<Event> = dates
        .iter()
        .map(|secs| keyed(type_, derived_payload(type_), &key, *secs))
        .collect();
    Namespaced::new(backend, project)
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed one derived type");
}

/// The project's run stream, read back through the SAME namespaced decorator that wrote it - so
/// what gets folded below is the log as a reader sees it, positions and valid-times included.
fn read_run_stream(backend: &Store, project: &str) -> Vec<Event> {
    Namespaced::new(backend, project)
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .expect("read the run stream back")
}

/// The LIVE graph `events` fold to, with the bitemporal `valid_from` and the `source` position
/// INCLUDED.
///
/// `graph_rows` above deliberately omits both: it answers "did `--runs` disturb the graph", where
/// a date is not the question. Here the date IS the question - it is the single column a
/// carry-forward writes and the single column a wrongly-placed type would move - so a comparison
/// that dropped it would be satisfied by exactly the defect this section exists to catch.
fn fold_live_with_dates(
    events: &[Event],
    project: &str,
    path: &Path,
) -> (Vec<String>, Vec<String>) {
    {
        let p = Projector::open(path.to_str().unwrap(), project).expect("open the context graph");
        p.apply_batch(events).expect("fold the log");
    }
    let conn = rusqlite::Connection::open(path).expect("open the context graph");
    let mut nodes: Vec<String> = conn
        .prepare("SELECT id, kind, COALESCE(attrs,''), project FROM nodes")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    nodes.sort();
    let mut edges: Vec<String> = conn
        .prepare(
            "SELECT from_id, to_id, rel, valid_from, source, project, tier FROM edges \
             WHERE valid_to IS NULL",
        )
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    edges.sort();
    (nodes, edges)
}

/// The two derived halves as sets: the types whose recordings RE-ASSERT (whose earliest date the
/// prune must carry) and the types whose recordings SUPERSEDE (whose surviving recording's own
/// date is already the live one).
fn derived_halves() -> (Vec<&'static str>, Vec<&'static str>) {
    let reasserted = rigger::ingest::reasserted_derived_types();
    let superseding: Vec<&'static str> = rigger::ingest::DERIVED_INDEX_TYPES
        .into_iter()
        .filter(|t| rigger::contextgraph::refold_supersedes_prior_edges(t))
        .collect();
    (reasserted, superseding)
}

#[test]
fn the_carry_forward_partition_is_the_folds_own_and_each_type_compacts_to_the_graph_it_folded() {
    let (reasserted, superseding) = derived_halves();

    // THE PARTITION IS TOTAL, DISJOINT, AND DERIVED. `reasserted_derived_types` is documented as
    // the derived-index types the fold predicate does NOT place in the superseding half - never a
    // second hand-written list - so a fifth derived type lands in exactly one half by construction.
    let all: BTreeSet<&str> = rigger::ingest::DERIVED_INDEX_TYPES.into_iter().collect();
    let left: BTreeSet<&str> = reasserted.iter().copied().collect();
    let right: BTreeSet<&str> = superseding.iter().copied().collect();
    assert!(
        left.is_disjoint(&right),
        "no derived type may be in both halves; both: {:?}",
        left.intersection(&right).collect::<Vec<_>>()
    );
    assert_eq!(
        left.union(&right).copied().collect::<BTreeSet<&str>>(),
        all,
        "every derived index type must be placed by the fold predicate; unplaced: {:?}",
        all.difference(&left.union(&right).copied().collect())
            .collect::<Vec<_>>()
    );
    assert!(
        !left.is_empty() && !right.is_empty(),
        "both halves must be inhabited, or the inversion below is not an inversion (reasserted \
         {left:?}, superseding {right:?})"
    );
    assert_eq!(
        left,
        all.iter()
            .copied()
            .filter(|t| !rigger::contextgraph::refold_supersedes_prior_edges(t))
            .collect::<BTreeSet<&str>>(),
        "reasserted_derived_types must be exactly the derived types the fold predicate does not \
         place in the superseding half"
    );

    // AND THE PARTITION IS THE FOLD'S OWN, type by type.
    const PROJECT: &str = "fold-partition";
    const EARLY: u64 = 1_000;
    const LATE: u64 = 2_000;
    let prefix = Namespaced::prefix_for(PROJECT);
    let scratch = tempfile::tempdir().expect("create a scratch dir");
    let mut folds_a_live_edge: BTreeSet<&str> = BTreeSet::new();

    for type_ in rigger::ingest::DERIVED_INDEX_TYPES {
        // Three identically seeded logs: one left whole, one pruned by the SHIPPED policy, one
        // pruned by the INVERTED one. Separate files, so each prune sees a log in its seeded state.
        let mut folded = Vec::new();
        for (case, reasserted_arg) in [
            ("whole", None),
            ("shipped", Some(reasserted.clone())),
            ("inverted", Some(superseding.clone())),
        ] {
            let db = scratch.path().join(format!("{type_}-{case}.db"));
            let backend = Store::open(db.to_str().unwrap()).expect("open the event log");
            seed_one_derived_type(&backend, PROJECT, type_, &[EARLY, LATE]);
            if let Some(reasserted_arg) = reasserted_arg {
                // The SHIPPED policy with its declaration replaced - the partition lives on the
                // policy value, so an inverted partition is an inverted policy, not a second
                // argument that could disagree with the one the policy already carries.
                let policy =
                    rigger::ingest::derived_index_identity().with_reasserting_types(reasserted_arg);
                let report = backend
                    .prune_derived_index(&prefix, &policy)
                    .expect("prune the derived index");
                assert_eq!(
                    report.total_removed(),
                    1,
                    "the {case} prune of {type_} must shed exactly the earlier recording, or the \
                     comparison below is not about a compaction at all"
                );
            }
            let events = read_run_stream(&backend, PROJECT);
            folded.push(fold_live_with_dates(
                &events,
                PROJECT,
                &scratch.path().join(format!("{type_}-{case}-graph.db")),
            ));
        }
        let (whole, shipped, inverted) = (&folded[0], &folded[1], &folded[2]);

        assert!(
            !whole.0.is_empty(),
            "{type_} must fold to something, or nothing below proves anything"
        );
        // THE COMPACTION CONTRACT, per type: the log the SHIPPED policy leaves folds to the graph
        // the whole log folds to - the same nodes, and the same live edges down to the assertion
        // date and the provenance position.
        assert_eq!(
            shipped.0, whole.0,
            "a {type_} log compacted by the shipped policy must fold to the same nodes"
        );
        assert_eq!(
            shipped.1, whole.1,
            "a {type_} log compacted by the shipped policy must fold to the same live edges, with \
             the same valid-from and the same provenance"
        );

        if whole.1.is_empty() {
            // A type whose fold writes NODES ONLY has no bitemporal column for a carry to move, so
            // its placement cannot re-date anything. That is a fact worth stating rather than a
            // case worth skipping: it is precisely why the equality above is not enough on its own,
            // and why the inversion below is asserted only where a date actually exists.
            assert_eq!(
                inverted.0, whole.0,
                "{type_} folds no live edge, so neither placement may change what it folds"
            );
            continue;
        }
        folds_a_live_edge.insert(type_);
        // AND THE PLACEMENT IS LOAD-BEARING. Swap the two halves and the live graph MOVES: the
        // re-asserting half loses the date the fact first became true, the superseding half gains
        // a date its fold retired. Without this, the equality above would hold for a partition
        // that said nothing at all.
        assert_ne!(
            inverted.1, whole.1,
            "putting {type_} in the WRONG half must move the live graph, or its placement is not \
             load-bearing and the equality above proves nothing"
        );
    }

    // The inversion has to have exercised BOTH directions, or half the partition is unproven.
    for (half, name) in [(&left, "re-asserting"), (&right, "superseding")] {
        assert!(
            half.iter().any(|t| folds_a_live_edge.contains(t)),
            "the {name} half must contain at least one type whose fold writes a live edge, or its \
             placement was never actually put to the test; edge-folding types: {folds_a_live_edge:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// 16. The API edges of the carry-forward itself.
//
// The content-identity policy now DECLARES the valid-time partition, and the store applies that
// declaration AS DATA: it holds no fold knowledge and asks, per covered type, whether the policy
// declared it re-asserting. That makes the declaration the whole contract, and it has edges the
// command's one shipped call site never reaches - declaring nothing at all, declaring some other
// covered type, and the key recorded exactly once, whose group has no earlier recording to
// inherit from and which the `HAVING COUNT(*) > 1` guard must therefore leave byte-identical.
// All of them are silent when wrong: a spurious UPDATE re-dates a fact and leaves every row
// looking perfectly intact. (The two declarations that cannot be acted on at all - undeclared,
// and declaring a type the policy does not cover - are refusals, pinned in section 2.)
// ---------------------------------------------------------------------------------------

/// Every row's `valid_from`, keyed by position - the ONE column the compaction is allowed to
/// write. Read separately from `raw_rows` on purpose: `raw_rows` is what "untouched" means for
/// every other column, so holding the two apart lets a test say "these rows are identical AND
/// exactly these dates moved" rather than conflating the two claims in one tuple.
fn valid_from_by_position(db: &Path) -> BTreeMap<i64, i64> {
    let conn = rusqlite::Connection::open(db).expect("open the event log");
    let mut stmt = conn
        .prepare("SELECT position, valid_from FROM events ORDER BY position")
        .unwrap();
    let out = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    out
}

/// Every valid-time in a log, addressed by what the row IS - its type and its replay key - rather
/// than by a position a fixture would have to hard-code. A key maps to a LIST because a bloated log
/// is precisely one that holds several recordings of one key.
type DatesByKey = BTreeMap<(String, String), Vec<i64>>;

/// One pruned fixture: the case that pruned it, its surviving rows, and the dates they carry.
type PrunedCase = (&'static str, Vec<Row>, DatesByKey);

/// The valid-time a row carries, addressed by what the row IS (its type and its replay key)
/// rather than by a position a fixture would have to hard-code.
fn dates_by_key(db: &Path) -> DatesByKey {
    let dates = valid_from_by_position(db);
    let mut out: DatesByKey = BTreeMap::new();
    for row in raw_rows(db) {
        let key = replay_key(&row).unwrap_or_default();
        out.entry((row.2.clone(), key))
            .or_default()
            .push(dates[&row.0]);
    }
    out
}

/// The one seed every carry-forward edge case below is pruned from: a re-recorded key from EACH
/// half, a key of the re-asserting half recorded exactly ONCE, and a non-derived event carrying a
/// derived-looking replay key.
fn seed_carry_forward_fixture(db: &Path, project: &str) {
    let backend = Store::open(db.to_str().unwrap()).expect("open the event log");
    let store = Namespaced::new(&backend, project);
    let mut events = vec![Event::new(
        "DecisionMade",
        br#"{"id":"d1","summary":"s","governs":["src/a.rs"],"supersedes":""}"#.to_vec(),
    )
    .with_meta(rigger::ingest::META_REPLAY_KEY, CARRY_KEY_DOC)
    .with_valid_from(UNIX_EPOCH + Duration::from_secs(500))];
    for secs in [1_000, 1_500, 2_000] {
        events.push(keyed(
            rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED,
            derived_payload(rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED),
            CARRY_KEY_DOC,
            secs,
        ));
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            derived_payload(rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED),
            CARRY_KEY_CODE,
            secs,
        ));
    }
    events.push(keyed(
        rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED,
        derived_payload(rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED),
        CARRY_KEY_SOLO,
        3_000,
    ));
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed the carry-forward fixture");
}

const CARRY_KEY_DOC: &str = "gd/src/a.rs@h1#0";
const CARRY_KEY_CODE: &str = "gc/src/a.rs@h1#0";
const CARRY_KEY_SOLO: &str = "gd/src/solo.rs@h1#0";

#[test]
fn the_carry_forward_touches_only_the_survivors_of_the_types_the_caller_named() {
    const PROJECT: &str = "carry-edges";
    let prefix = Namespaced::prefix_for(PROJECT);
    let scratch = tempfile::tempdir().expect("create a scratch dir");
    let (reasserted, _) = derived_halves();
    let doc = rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED.to_string();
    let code = rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED.to_string();
    assert!(
        reasserted.contains(&doc.as_str()) && !reasserted.contains(&code.as_str()),
        "this fixture reads a type from each half; the shipped partition put them both in {reasserted:?}"
    );

    // Three prunes of one identical seed, differing ONLY in what the policy declares as
    // re-asserting. Every one of them is a declaration a caller may truthfully make; the two
    // declarations that cannot be acted on are refused before any row is read (section 2).
    let cases: [(&str, Vec<&str>); 3] = [
        // A caller stating that NONE of its types re-assert. It writes no column at all, so every
        // survivor keeps the date it was recorded with. Not the same state as an undeclared
        // policy, which is refused: this one is an answer.
        ("declared-nothing", Vec::new()),
        // The shipped answer.
        ("declared-shipped", reasserted.clone()),
        // A DIFFERENT covered type, re-asserting like the doc half but absent from this fixture.
        // The store iterates its own covered types and asks about each ONE BY NAME, so declaring
        // a sibling leaves this fixture's types exactly where an empty declaration leaves them -
        // whereas an implementation that instead asked "did the policy declare anything" would
        // carry every type on this input.
        (
            "declared-other-covered-type",
            vec![rigger::contextgraph::TYPE_DOC_CONCEPT_EXTRACTED],
        ),
    ];

    let mut observed: Vec<PrunedCase> = Vec::new();
    let mut seeded: Option<(Vec<Row>, DatesByKey)> = None;
    for (case, reasserted_arg) in cases {
        let db = scratch.path().join(format!("{case}.db"));
        seed_carry_forward_fixture(&db, PROJECT);
        let before = (shape(&raw_rows(&db)), dates_by_key(&db));
        match &seeded {
            None => seeded = Some((raw_rows(&db), dates_by_key(&db))),
            Some(first) => assert_eq!(
                (shape(&first.0), first.1.clone()),
                before,
                "every case must start from an identical log, or their outcomes are not comparable"
            ),
        }
        let backend = Store::open(db.to_str().unwrap()).expect("open the event log");
        let policy =
            rigger::ingest::derived_index_identity().with_reasserting_types(reasserted_arg);
        backend
            .prune_derived_index(&prefix, &policy)
            .expect("prune the derived index");
        observed.push((case, raw_rows(&db), dates_by_key(&db)));
    }

    // WHICH ROWS SURVIVE IS THE SAME IN ALL THREE. The carry argument is about DATES; a policy
    // value that changed the selection would be a different command wearing the same name.
    let baseline = shape(&observed[0].1);
    for (case, rows, _) in &observed[1..] {
        assert_eq!(
            shape(rows),
            baseline,
            "the {case} prune must delete exactly the rows every other prune of this seed \
             deletes - the declaration names dates to carry, never rows to drop"
        );
    }

    let survivor_date = |dates: &DatesByKey, type_: &str, key: &str| {
        let got = dates
            .get(&(type_.to_string(), key.to_string()))
            .unwrap_or_else(|| panic!("the survivor of {type_} / {key} must still be in the log"));
        assert_eq!(
            got.len(),
            1,
            "exactly one recording of {type_} / {key} may survive; got {got:?}"
        );
        got[0]
    };
    let secs = |s: u64| Duration::from_secs(s).as_nanos() as i64;

    for (case, _, dates) in &observed {
        // THE SUPERSEDING HALF IS NEVER CARRIED under any of these declarations: its surviving
        // recording's own date is the one its fold arrives at.
        assert_eq!(
            survivor_date(dates, &code, CARRY_KEY_CODE),
            secs(2_000),
            "the {case} prune must leave the superseding half's survivor at its own recorded date"
        );
        // A KEY RECORDED ONCE IS NEVER REWRITTEN. It has no earlier recording to inherit from, and
        // the `n > 1` guard is what keeps the carry off a row the prune did not touch.
        assert_eq!(
            survivor_date(dates, &doc, CARRY_KEY_SOLO),
            secs(3_000),
            "the {case} prune must leave a singly-recorded key exactly as it found it"
        );
        // THE NON-DERIVED EVENT IS NEVER READ, WRITTEN, OR MOVED - however its replay key is
        // spelled. It shares a key with the derived row the carry rewrites, so an UPDATE that
        // matched on the key rather than on the key AND the type would re-date a domain event.
        assert_eq!(
            survivor_date(dates, "DecisionMade", CARRY_KEY_DOC),
            secs(500),
            "the {case} prune must never touch a non-derived event, however its key is spelled"
        );
    }

    // AND THE RE-ASSERTING HALF IS CARRIED EXACTLY WHEN THE CALLER NAMED IT.
    let doc_dates: Vec<(&str, i64)> = observed
        .iter()
        .map(|(case, _, dates)| (*case, survivor_date(dates, &doc, CARRY_KEY_DOC)))
        .collect();
    assert_eq!(
        doc_dates,
        vec![
            ("declared-nothing", secs(2_000)),
            ("declared-shipped", secs(1_000)),
            ("declared-other-covered-type", secs(2_000)),
        ],
        "the earliest date must be carried onto the survivor when - and only when - the policy \
         declared its type re-asserting; got {doc_dates:?}"
    );
}

// ---------------------------------------------------------------------------------------
// 17. The store the prune migrates is the store the prune addresses.
//
// `reset` now runs the spec-09 identity migration before any prune, because a log bloated enough
// to need compacting is by construction an OLD log whose streams were written under the
// pre-identity basename namespace. That migration is anchored at the RESOLVED store's owning root
// - not at the process cwd - and the two anchors are indistinguishable from the project root,
// where every other test of this unit runs.
//
// They come apart exactly where the store walk exists for: a nested WORKTREE. Its own git
// top-level is itself, so a cwd-anchored migration computes both halves of its comparison from
// the worktree (which has no minted identity, so they agree and it migrates nothing) while the
// prune addresses the store it walked UP to - and reports a perfectly successful prune of zero
// rows against a log whose history is all still under the legacy namespace. That is the silent
// no-op this command's whole design refuses, and no fixture rooted at the project can see it.
// ---------------------------------------------------------------------------------------

/// The durable identity the fixtures below mint. A fixed string rather than a derivation of the
/// basename it replaces, for the reason spelled out where it is written.
const MINTED_ID: &str = "compaction-fixture-9f2c1a";

/// A project whose event log was written under the LEGACY basename namespace and which only
/// afterwards minted a durable identity - the shape a store that needs compacting actually has.
/// Returns `(legacy identity, minted identity)`.
fn seed_project_under_the_legacy_namespace(root: &Path, rounds: u64) -> (String, String) {
    git_in(root, &["init", "-q"]);
    git_in(root, &["config", "user.email", "fixture@example.invalid"]);
    git_in(root, &["config", "user.name", "fixture"]);
    git_in(root, &["commit", "-q", "--allow-empty", "-m", "seed"]);
    std::fs::create_dir_all(root.join(".rigger")).expect("create .rigger");

    // Seeded BEFORE the mint, so the history is filed under the basename namespace exactly as a
    // pre-identity store's is. A fixture that minted first would prove nothing about the migration.
    let legacy = project_identity(root);
    let backend = Store::open(event_log(root).to_str().unwrap()).expect("open the event log");
    seed_namespace(&backend, &legacy, rounds);
    drop(backend);

    // The minted id shares NO prefix with the basename it replaces. `proj-<id>-` is a separator-free
    // string prefix, so an id of the form `<legacy>-<suffix>` would leave every migrated stream
    // still matching the LEGACY prefix and the assertions below would be reading that ambiguity
    // rather than whether the history moved.
    let minted = MINTED_ID.to_string();
    std::fs::write(root.join(".rigger").join("project.id"), &minted).expect("mint the identity");
    assert!(
        !minted.starts_with(&legacy) && !legacy.starts_with(&minted),
        "neither identity may be a string prefix of the other, or the namespace assertions below \
         cannot tell a migrated stream from an unmigrated one ({legacy:?} vs {minted:?})"
    );
    assert_ne!(
        legacy,
        project_identity(root),
        "the mint must produce an identity distinct from the basename, or this fixture does not \
         reproduce the shape it exists for"
    );
    (legacy, minted)
}

#[test]
fn a_reset_from_a_nested_worktree_migrates_and_compacts_the_store_it_walked_up_to() {
    const ROUNDS: u64 = 4;
    let removed_per_key = (ROUNDS - 1) as usize;

    // Two identically seeded projects. One is reset from its own root, the other from a worktree
    // nested inside it: the SAME store, reached by the SAME walk, from two different working
    // directories. Whatever the command does, it must do the same thing in both.
    let from_root = tempfile::tempdir().expect("create a temp project");
    let from_worktree = tempfile::tempdir().expect("create a temp project");
    let (legacy_a, minted_a) = seed_project_under_the_legacy_namespace(from_root.path(), ROUNDS);
    let (legacy_b, minted_b) =
        seed_project_under_the_legacy_namespace(from_worktree.path(), ROUNDS);

    let nested = from_worktree.path().join("wt");
    git_in(
        from_worktree.path(),
        &["worktree", "add", "-q", "--detach", "wt"],
    );
    assert!(
        !nested.join(".rigger").exists(),
        "the nested worktree must carry no store of its own, or the walk would stop at a shadow \
         instead of reaching the project's store"
    );

    let before_a = raw_rows(&event_log(from_root.path())).len();
    let before_b = raw_rows(&event_log(from_worktree.path())).len();
    assert_eq!(
        before_a, before_b,
        "the two fixtures must start from logs of the same size"
    );

    let (out_a, err_a, ok_a) = run_rigger(from_root.path(), &["reset", "--derived"]);
    assert!(
        ok_a,
        "reset --derived at the root must succeed: {err_a}\n{out_a}"
    );
    let (out_b, err_b, ok_b) = run_rigger(&nested, &["reset", "--derived"]);
    assert!(
        ok_b,
        "reset --derived from a nested worktree must succeed - the store walk reaches the \
         project's store from there: {err_b}\n{out_b}"
    );

    // THE PRUNE DID THE WORK, from the worktree exactly as from the root. A zero-row report here
    // is the failure this test exists for: it is what a migration anchored at the process cwd
    // produces, and it is indistinguishable from a healthy prune of an already-compacted log
    // unless the counts are asserted.
    for (where_, out) in [("the root", &out_a), ("a nested worktree", &out_b)] {
        let report = per_type_report(out);
        let expected: Vec<(String, usize)> = rigger::ingest::DERIVED_INDEX_TYPES
            .into_iter()
            .map(|t| {
                let n = if t == rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED
                    || t == rigger::contextgraph::TYPE_EDGE_INFERRED
                {
                    removed_per_key
                } else {
                    0
                };
                (t.to_string(), n)
            })
            .collect();
        assert_eq!(
            report, expected,
            "reset --derived run from {where_} must compact the legacy-namespaced history it \
             walked up to; got {out:?}"
        );
    }

    // AND THE MIGRATION MOVED THE HISTORY rather than leaving it stranded: nothing survives under
    // the legacy namespace, and everything survives under the minted one.
    for (where_, root, legacy, minted, before) in [
        ("the root", from_root.path(), &legacy_a, &minted_a, before_a),
        (
            "a nested worktree",
            from_worktree.path(),
            &legacy_b,
            &minted_b,
            before_b,
        ),
    ] {
        let after = raw_rows(&event_log(root));
        assert!(
            after.len() < before,
            "reset --derived from {where_} must actually shed rows: {before} before, {} after",
            after.len()
        );
        assert!(
            rows_in(&after, &Namespaced::prefix_for(legacy)).is_empty(),
            "no row may be left behind under the legacy namespace after a reset from {where_}"
        );
        assert_eq!(
            rows_in(&after, &Namespaced::prefix_for(minted)).len(),
            after.len(),
            "every surviving row must live under the minted namespace after a reset from {where_}"
        );
    }

    // The two invocations are the SAME operation: one store, one authority, two cwds.
    assert_eq!(
        shape(&raw_rows(&event_log(from_root.path())))
            .iter()
            .map(|r| (r.0, r.2.clone(), r.5))
            .collect::<Vec<_>>(),
        shape(&raw_rows(&event_log(from_worktree.path())))
            .iter()
            .map(|r| (r.0, r.2.clone(), r.5))
            .collect::<Vec<_>>(),
        "a reset from a nested worktree must leave the log in the state a reset from the root \
         leaves it in"
    );
}

/// `rigger reset --runs` runs the identity migration too, and its printed report has to be true
/// of THAT, not just of the graph prune underneath it.
///
/// The migration is wired into `cmd_reset` for every sqlite invocation rather than into
/// `--derived` alone, and it must be: `reset_runs` reads through the project's MINTED namespace,
/// so on a store still filed under the legacy basename it would read an empty stream and report a
/// confident prune of zero dead-run nodes. But that makes `--runs` a command that writes the
/// event log on exactly the old-store class it is most likely to be pointed at, and the report
/// used to end "the event log is untouched". This pins the corrected claim against the store the
/// claim is about, both halves at once: NOTHING IS DELETED (every seeded row survives with its
/// position, type, id, payload bytes and revision intact, only its stream re-namespaced) and
/// EXACTLY ONE EVENT IS APPENDED (the migration's own `DecisionMade`) - and the printed line says
/// both rather than promising an untouched file.
#[test]
fn reset_runs_alone_migrates_a_legacy_store_and_its_report_says_what_that_wrote() {
    const ROUNDS: u64 = 3;
    let project = tempfile::tempdir().expect("create a temp project");
    let (legacy, minted) = seed_project_under_the_legacy_namespace(project.path(), ROUNDS);
    let legacy_ns = Namespaced::prefix_for(&legacy);
    let minted_ns = Namespaced::prefix_for(&minted);

    let before = raw_rows(&event_log(project.path()));
    assert!(
        !rows_in(&before, &legacy_ns).is_empty() && rows_in(&before, &minted_ns).is_empty(),
        "the premise: this store's whole history is under the LEGACY namespace, which is the only \
         shape on which reset writes the log at all"
    );

    let (out, err, ok) = run_rigger(project.path(), &["reset", "--runs"]);
    assert!(ok, "reset --runs must succeed; stderr: {err}\n{out}");

    // NOTHING WAS DELETED, and nothing was renumbered or re-dated: each seeded row is still there,
    // in order, with only its stream moved into the minted namespace. That is the half of the
    // claim an operator cannot check afterwards, so it is checked here column by column.
    let after = raw_rows(&event_log(project.path()));
    let carried: Vec<Row> = before
        .iter()
        .map(|r| {
            let suffix =
                r.1.strip_prefix(&legacy_ns)
                    .expect("every seeded row is under the legacy namespace");
            (
                r.0,
                format!("{minted_ns}{suffix}"),
                r.2.clone(),
                r.3.clone(),
                r.4.clone(),
                r.5.clone(),
                r.6,
            )
        })
        .collect();
    assert!(
        after.starts_with(&carried),
        "reset --runs deletes no event: every row must survive with its position, type, id, \
         payload and revision, renamed into the minted namespace and nothing more. It differs at \
         {}; the command said: {out:?}",
        first_difference(&row_marks(&after), &row_marks(&carried))
    );

    // AND EXACTLY ONE EVENT WAS APPENDED: the migration's record of itself, in the minted
    // namespace. One, not zero (the migration really did fire on this store) and not two (nothing
    // else about `--runs` writes the log).
    let appended = &after[carried.len()..];
    assert_eq!(
        appended.len(),
        1,
        "the migration records itself with ONE event and the graph prune writes none, so a \
         legacy-store `reset --runs` appends exactly one row; it appended {:?}",
        appended.iter().map(|r| (&r.1, &r.2)).collect::<Vec<_>>()
    );
    assert_eq!(
        appended[0].2,
        rigger::contextgraph::TYPE_DECISION_MADE,
        "and it is the existing DecisionMade the migration is recorded with - no event type is \
         minted for it; got {:?}",
        appended[0].2
    );
    assert!(
        appended[0].1.starts_with(&minted_ns),
        "recorded under the identity the history was migrated TO, or the audit trail of the \
         migration is filed where the migration moved the history away from; got {:?}",
        appended[0].1
    );
    assert!(
        rows_in(&after, &legacy_ns).is_empty(),
        "and the legacy namespace is empty afterwards, or the migration did not complete"
    );

    // THE REPORT SAYS SO. The old sentence promised an untouched event log two lines under a
    // command that had just renamed every stream in it.
    assert!(
        !out.contains("the event log is untouched"),
        "the report must not promise an untouched log on the one store class where reset writes \
         it; got {out:?}"
    );
    for (fact, needle) in [
        ("say the prune itself deletes nothing", "deletes no event"),
        ("name the one thing reset does write", "identity migration"),
        ("name what that migration records", "DecisionMade"),
        (
            "tell the operator how to see it happen",
            "prints its own line",
        ),
    ] {
        assert!(
            out.contains(needle),
            "the reset --runs report must {fact} ({needle:?}); got {out:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// 18. The key split the policy publishes.
//
// `ContentIdentity::split` exists so a caller needing the same key form under a different metadata
// key builds a VARIANT of the shipped policy instead of writing a second parser of that form. The
// property that is worth anything is that the variant parses IDENTICALLY - an accessor that
// returned some other function would still typecheck, still compile every call site, and quietly
// give the guard and the compaction two different opinions about where a key's generation begins.
// ---------------------------------------------------------------------------------------

#[test]
fn the_published_key_split_is_the_policys_own_and_a_variant_policy_parses_identically() {
    let shipped = rigger::ingest::derived_index_identity();
    let split = shipped.split();
    // A variant under a different metadata key and a narrower type list, built the only way the
    // accessor is meant to be used.
    let variant = ContentIdentity::new(
        "some_other_meta_key",
        vec![rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED],
        split,
    );

    // Well-formed keys, including the shapes the key form deliberately allows: a file path that
    // itself contains the `/`, `@` and `#` the key uses as separators.
    for key in [
        "gc/src/a.rs@h1#0",
        "gd/docs/design.md@abcdef0123456789#12",
        "gc/src/od/d@ta#1.rs@deadbeef#3",
    ] {
        let via_accessor: Option<(Range<usize>, Range<usize>)> = split(key);
        assert_eq!(
            via_accessor,
            shipped.split_of(key),
            "the published split must be the split the policy itself reads {key:?} with"
        );
        assert_eq!(
            variant.split_of(key),
            shipped.split_of(key),
            "a policy built from the published split must parse {key:?} identically"
        );
        let (identity, generation) = shipped
            .split_of(key)
            .unwrap_or_else(|| panic!("{key:?} is a well-formed content key"));
        // The ranges name the substrings the key form promises: the subject up to the `@`, and the
        // content generation between the `@` and the `#<i>` tail.
        let subject = key
            .rsplit_once('#')
            .expect("a well-formed key has a #<i> tail")
            .0;
        let (before_at, hash) = tail_free(subject);
        assert_eq!(
            &key[identity], before_at,
            "the identity range must be the batch subject"
        );
        assert_eq!(
            &key[generation], hash,
            "the generation range must be the content hash"
        );
    }

    // And a string that is not that shape parses to nothing, through both spellings alike.
    for key in [
        "",
        "no-slash@h1#0",
        "gc/src/a.rs@h1",
        "gc/src/a.rs@h1#x",
        "/src/a.rs@h1#0",
    ] {
        assert!(
            split(key).is_none()
                && shipped.split_of(key).is_none()
                && variant.split_of(key).is_none(),
            "{key:?} is not a content key and must parse to nothing through every spelling"
        );
    }
}

/// A well-formed key's subject and content hash, split from the RIGHT at the last `@` - the same
/// direction the key authority splits, because a file path may itself contain one.
fn tail_free(subject: &str) -> (&str, &str) {
    subject
        .rsplit_once('@')
        .expect("a well-formed key carries an @ before its tail")
}

// ---------------------------------------------------------------------------------------
// 19. The SHIPPED policy's OWN declaration - the one every other carry test replaces.
//
// The valid-time partition is data the store applies without understanding it, so the value that
// arrives at the one production call site is the whole of the property. Section 16 proves the
// store applies whatever it is told, and it proves that by handing the store a declaration each
// case CHOSE (`derived_index_identity().with_reasserting_types(<the case's list>)`). Section 15
// proves `ingest::reasserted_derived_types` is the fold's own partition. Neither of them ever
// reads what `derived_index_identity` DECLARED, and the refusal in section 2 only separates
// declared from undeclared.
//
// So the wiring between the two - the `.with_reasserting_types(reasserted_derived_types())` that
// puts the fold's answer ONTO the shipped policy - is load-bearing and, until this test, unheld.
// Its failure mode is the quietest one this unit has: an EMPTY declaration is honored by design
// (a caller may truthfully say that none of its types re-assert), so a shipped policy that
// declared nothing would be accepted, would delete exactly the rows it deletes today, and would
// re-date every design fact in the log to whichever recording survived - with the graph still
// folding, every row still intact, and every other test in this file and its sibling still green.
// ---------------------------------------------------------------------------------------

#[test]
fn the_shipped_policy_declares_the_fold_s_own_partition_and_the_prune_applies_that_declaration() {
    let shipped = rigger::ingest::derived_index_identity();

    // IT DECLARED AT ALL - otherwise the one production call site is refused outright, which is
    // the loud failure rather than the silent one.
    let declared: BTreeSet<&str> = shipped
        .reasserting()
        .expect(
            "the shipped derived-index policy must declare its valid-time partition: the command \
             that uses it is refused without one",
        )
        .iter()
        .map(String::as_str)
        .collect();

    // AND IT DECLARED THE FOLD'S OWN PARTITION, not merely something. Compared against the fold
    // predicate directly rather than against `reasserted_derived_types`, so this cannot be
    // satisfied by two helpers in `ingest` agreeing with each other while the projection means
    // something else.
    let implied: BTreeSet<&str> = rigger::ingest::DERIVED_INDEX_TYPES
        .into_iter()
        .filter(|t| !rigger::contextgraph::refold_supersedes_prior_edges(t))
        .collect();
    assert_eq!(
        declared, implied,
        "the shipped policy must declare exactly the derived types whose fold RE-ASSERTS a fact \
         in place; anything else re-dates or back-dates facts through the shipped command"
    );
    assert!(
        !declared.is_empty() && declared.len() < rigger::ingest::DERIVED_INDEX_TYPES.len(),
        "both halves must be inhabited on the SHIPPED value, or the declaration below is not \
         actually distinguishing anything; declared {declared:?} of {:?}",
        rigger::ingest::DERIVED_INDEX_TYPES
    );

    // PER TYPE, THROUGH THE ACCESSOR THE STORE ITSELF CONSULTS. `reasserting()` is the
    // declaration; `reasserts()` is the question the prune asks about one type at a time, and a
    // membership test that disagreed with the list would be invisible to the set comparison above.
    for type_ in rigger::ingest::DERIVED_INDEX_TYPES {
        assert_eq!(
            shipped.reasserts(type_),
            Some(!rigger::contextgraph::refold_supersedes_prior_edges(type_)),
            "the shipped policy must answer for {type_} exactly as the fold does"
        );
    }

    // THE THIRD STATE IS STILL A THIRD STATE. The same types, asked of a policy that was never
    // told the partition, answer `None` - not `Some(false)`. That is what makes the refusal in
    // section 2 reachable at all, and it is the distinction a `bool` return would erase.
    let undeclared = ContentIdentity::new(
        rigger::ingest::META_REPLAY_KEY,
        rigger::ingest::DERIVED_INDEX_TYPES,
        shipped.split(),
    );
    for type_ in rigger::ingest::DERIVED_INDEX_TYPES {
        assert_eq!(
            undeclared.reasserts(type_),
            None,
            "an undeclared policy must answer NEITHER re-asserting nor superseding for {type_}, \
             or a compaction cannot tell 'nothing re-asserts' from 'nobody said'"
        );
    }

    // AND THE DECLARATION IS THE ONE THAT REACHES THE ROWS. Everything above is about a value; the
    // property is about dates in a file. This prunes with the shipped policy EXACTLY as the
    // command hands it over - no `with_reasserting_types` in the way - and reads the dates back
    // out of the compacted log.
    const PROJECT: &str = "shipped-declaration";
    let dir = tempfile::tempdir().expect("create a scratch dir");
    let db = dir.path().join("shipped-declaration.db");
    seed_carry_forward_fixture(&db, PROJECT);
    let backend = Store::open(db.to_str().unwrap()).expect("open the event log");
    let pruned = backend
        .prune_derived_index(&Namespaced::prefix_for(PROJECT), &shipped)
        .expect("the shipped policy must be one the store can act on");
    assert!(
        pruned.total_removed() > 0,
        "the fixture holds duplicated recordings, so the shipped policy must shed them; got \
         {pruned:?}"
    );

    let secs = |s: u64| Duration::from_secs(s).as_nanos() as i64;
    let dates = dates_by_key(&db);
    let survivor = |type_: &str, key: &str| -> i64 {
        let got = dates
            .get(&(type_.to_string(), key.to_string()))
            .unwrap_or_else(|| panic!("the survivor of {type_} / {key} must still be in the log"));
        assert_eq!(
            got.len(),
            1,
            "exactly one recording of {type_} / {key} may survive; got {got:?}"
        );
        got[0]
    };
    assert_eq!(
        survivor(rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED, CARRY_KEY_DOC),
        secs(1_000),
        "the RE-ASSERTING half's survivor must carry the date the fact first became true. It \
         holds its group's LATEST date when the shipped policy declares an empty or wrong \
         partition - which the store accepts, because an empty declaration is a truthful thing \
         for some caller to say"
    );
    assert_eq!(
        survivor(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            CARRY_KEY_CODE
        ),
        secs(2_000),
        "the SUPERSEDING half's survivor must keep its own recorded date; a shipped policy that \
         over-declared would drag it back to a date its fold retired"
    );
}

// ---------------------------------------------------------------------------------------
// 20. The report on a log with NOTHING TO SHED - a promise the shipped documents now make.
//
// The committed operator guidance states, in the paragraph section 7 pins word for word, that on a
// log holding no key twice `rigger reset --derived` deletes ZERO rows from it "and reports so",
// and that this is the expected report rather than a failure. That is a claim about the BINARY,
// made in a document, and the only thing that can hold the two together is a test that runs the
// binary on such a log.
//
// It is the case an operator is most likely to meet first and least able to interpret: a command
// that deletes from an append-only log printing `pruned 0` looks exactly like a command that
// found nothing because it was pointed at the wrong store, matched no stream, or silently failed
// - the failure this unit's own history already contains once. Nothing else in either suite
// reaches it: every other fixture seeds duplication on purpose, because shedding it is what the
// command is for.
//
// Both directions are asserted, because a sentence that always printed would satisfy the first
// half alone while telling an operator staring at a real prune that nothing was shed.
// ---------------------------------------------------------------------------------------

/// The clause the report adds when there was nothing to shed, split into the three things it has
/// to convey: that the state is understood, that it is the EXPECTED one, and that it is not a
/// failure. Held as separate needles so a rewording that drops one of the three is a failure that
/// names which one.
const NOTHING_TO_SHED: [(&str, &str); 3] = [
    ("say WHY nothing was shed", "no redundancy to shed"),
    ("say the report is the EXPECTED one", "expected report"),
    ("say it is not a failure", "not a failed prune"),
];

#[test]
fn a_log_with_nothing_to_shed_is_reported_as_the_expected_result_and_left_exactly_as_found() {
    // A CLEAN LOG: one recording per replay key, which is what a log written since the ingest
    // dedup existed holds. The fixture differs from every other one in this file by exactly the
    // round count, so "clean" here means precisely "no key recorded twice".
    let dir = temp_project();
    let root = dir.path();
    seed_project(root, 1);
    let before = raw_rows(&event_log(root));
    let dates_before = valid_from_by_position(&event_log(root));
    assert!(
        !before.is_empty(),
        "the fixture must hold events, or `pruned 0` would be true of an empty file instead of a \
         clean one"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(
        ok,
        "a log with nothing to shed is not an error: the command must succeed. stdout {out:?}, \
         stderr {err:?}"
    );
    assert!(
        out.contains("pruned 0 redundant derived-index event(s)"),
        "the command must report the count it actually removed; got {out:?}"
    );
    for (fact, needle) in NOTHING_TO_SHED {
        assert!(
            out.contains(needle),
            "the report on a clean log must {fact} ({needle:?}); `pruned 0` on its own reads as a \
             prune that found the wrong store. Got {out:?}"
        );
    }

    // EVERY DECLARED TYPE IS STILL ACCOUNTED FOR, at zero. The per-type list is what tells an
    // operator the command looked at each type rather than short-circuiting on an empty result.
    let per_type = per_type_report(&out);
    assert_eq!(
        per_type
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<&str>>(),
        rigger::ingest::DERIVED_INDEX_TYPES.to_vec(),
        "a zero prune must still account for every derived index type `ingest` declares; got \
         {per_type:?}"
    );
    assert!(
        per_type.iter().all(|(_, n)| *n == 0),
        "no type may report a removal on a log with no duplicated key; got {per_type:?}"
    );

    // AND THE LOG IS EXACTLY AS IT WAS FOUND. Not merely the same number of rows: the same rows,
    // and the same dates. The carry runs over the re-asserting types on every invocation, and a
    // carry that did not guard on a key being recorded more than once would rewrite `valid_from`
    // on rows this prune reported it had left alone - a mutation no row count can see.
    assert_eq!(
        raw_rows(&event_log(root)),
        before,
        "a prune that shed nothing must leave every row byte-for-byte, VACUUM included"
    );
    assert_eq!(
        valid_from_by_position(&event_log(root)),
        dates_before,
        "a prune that shed nothing must re-date nothing: the carry only ever rewrites the \
         survivor of a key that WAS recorded more than once"
    );

    // THE SHIPPED DOCUMENT PROMISED EXACTLY THIS, and the promise is only worth what the binary
    // does. Read from the committed bytes an operator opens, so the two cannot drift apart with
    // the renderer green.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for rel in SHIPPED_DOCS {
        let shipped = std::fs::read_to_string(manifest.join(rel))
            .unwrap_or_else(|e| panic!("the operator document {rel} must ship: {e}"));
        assert!(
            shipped.contains("deletes ZERO rows from it and reports so"),
            "the committed {rel} must tell an operator what a clean log reports, or the run above \
             is behavior nobody was promised"
        );
    }

    // THE OTHER DIRECTION: on a log that DOES hold duplication the clause is absent, so it reads
    // as a statement about this log rather than as boilerplate the command always prints.
    let bloated = temp_project();
    seed_project(bloated.path(), 4);
    let (out, err, ok) = run_rigger(bloated.path(), &["reset", "--derived"]);
    assert!(
        ok,
        "the bloated prune must succeed; stdout {out:?}, stderr {err:?}"
    );
    assert!(
        !out.contains("pruned 0 redundant derived-index event(s)"),
        "the bloated fixture must actually shed rows, or the contrast below is between two \
         identical cases; got {out:?}"
    );
    for (fact, needle) in NOTHING_TO_SHED {
        assert!(
            !out.contains(needle),
            "a prune that DID shed rows must not {fact} ({needle:?}): a clause that always prints \
             tells an operator watching a real compaction that there was nothing to compact. Got \
             {out:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// 21. The COMMAND's rendering of a reclamation it could not measure.
//
// Section 2 pins the store primitive: a reader parked on the write-ahead log makes the truncating
// checkpoint decline, the freed pages stay in the `-wal` file, and `reclaimed_bytes` is `None`
// rather than a page-count delta the operator's own `ls` would contradict. `None` is only half the
// honesty, though - the operator never sees an `Option`. They see one line of prose, and the arm
// that renders `None` is a different piece of code from the one that produced it, reachable only
// by running the shipped binary against a log somebody else is reading.
//
// That is not an exotic state. A second rigger process reading the log - a dashboard, a status
// call, a concurrent agent - is the ordinary condition of the store this command exists to
// compact, so the arm an operator is likeliest to hit is the one no test drives.
// ---------------------------------------------------------------------------------------

/// What the command says when the reclamation IS a measurement, and when it is not. The two are
/// asserted against each other rather than in isolation: each report must carry its own phrase and
/// NOT the other's, which is what separates "the command rendered the state it was in" from "the
/// command prints one of these regardless".
const MEASURED: &str = "byte(s) on disk";
const UNMEASURED: &str = "could not be folded back into the file";

#[test]
fn the_command_reports_an_unmeasurable_reclamation_as_unmeasured_rather_than_as_a_byte_count() {
    const ROUNDS: u64 = 6;

    // THE CONTENDED RUN. The reader is parked BEFORE the binary starts and released only after it
    // exits, so the checkpoint is refused for the whole of the command's life. An open read
    // transaction from a second connection is exactly what a second rigger process holds.
    let dir = temp_project();
    let root = dir.path();
    seed_project(root, ROUNDS);
    // ROOM TO RECLAIM. The rewrite runs over a file that is holding reclaimable free pages, and
    // the seeded duplication is a few small rows that can free no whole page at all - so without
    // this both runs below would honestly skip the rewrite and neither arm of the contrast would
    // be reached. Planted free pages change nothing about what the reclamation MEANS: the vacuum
    // reclaims the pages the file is not using, however they came to be free.
    plant_free_pages(&event_log(root), 3_000);
    let reader =
        rusqlite::Connection::open(event_log(root)).expect("open a second connection to the log");
    reader
        .execute_batch("BEGIN")
        .expect("begin the reader's transaction");
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("take a read snapshot");

    let (contended, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(
        ok,
        "a reader on the log does not fail the prune - the deletes commit before the checkpoint \
         is ever asked for. stdout {contended:?}, stderr {err:?}"
    );
    drop(reader);

    // THE CONTROL. A second project seeded identically, with nobody reading it.
    let solo_dir = temp_project();
    seed_project(solo_dir.path(), ROUNDS);
    plant_free_pages(&event_log(solo_dir.path()), 3_000);
    let (uncontended, err, ok) = run_rigger(solo_dir.path(), &["reset", "--derived"]);
    assert!(
        ok,
        "the uncontended prune must succeed; stdout {uncontended:?}, stderr {err:?}"
    );

    // THE DELETES ARE THE SAME. Whatever the reclamation says, a parked reader must not change
    // WHICH rows the command sheds, or the report below is describing two different prunes.
    assert_eq!(
        per_type_report(&contended),
        per_type_report(&uncontended),
        "a reader parked on the log must not change what the prune removes; contended \
         {contended:?}, uncontended {uncontended:?}"
    );
    assert!(
        per_type_report(&contended).iter().any(|(_, n)| *n > 0),
        "the fixture must actually shed rows, or neither report is about a prune that reclaimed \
         anything; got {contended:?}"
    );

    // AND EACH REPORT CARRIES ITS OWN PHRASE, AND ONLY ITS OWN.
    assert!(
        uncontended.contains(MEASURED) && !uncontended.contains(UNMEASURED),
        "with nobody holding the write-ahead log the checkpoint completes, so the command must \
         report the bytes it measured; got {uncontended:?}"
    );
    assert!(
        contended.contains(UNMEASURED) && !contended.contains(MEASURED),
        "while a reader held the write-ahead log the freed pages never left it, so the command \
         must say the reclamation was UNMEASURED rather than print a byte count the operator's \
         own `ls` contradicts; got {contended:?}"
    );
    assert!(
        contended.contains("next checkpoint"),
        "the unmeasured report must tell an operator where the freed pages went, or it reads as a \
         prune that failed to reclaim rather than one whose reclamation was deferred; got \
         {contended:?}"
    );
}

// ---------------------------------------------------------------------------------------
// 22. WHAT DECIDES WHETHER THE FILE IS REWRITTEN: the space there is to reclaim, not the rows
//     this pass deleted.
//
// Section 20 pins what the zero-delete prune SAYS. This pins what it COSTS, which is the half a
// row-and-date comparison cannot see: a full file rewrite leaves every row byte-for-byte too, so
// section 20 is satisfied by a command that vacuumed the entire log to reclaim nothing.
//
// The rewrite is the most expensive thing the command can do: it holds the write lock for a full
// scan of the log, stages a COMPLETE second copy of the database in the temporary directory
// SQLite resolves - which on the ordinary layout is a different, much smaller filesystem than the
// one holding `.rigger/` - and reclaims exactly the free pages the file is holding. So a file
// holding none must not be rewritten at all.
//
// BUT THE TRIGGER IS THE FREE SPACE, NOT THE DELETES, and that is the second half of this
// section. A rewrite gated on the rows THIS pass deleted skips the file whenever the deletes
// already happened - which is exactly the state a prune whose reclamation FAILED leaves behind,
// and exactly the state the failure report tells the operator to re-run out of. Gated that way
// the re-run deletes nothing, skips the rewrite, and reclaims nothing, forever. The two tests
// below are therefore one property from both sides: nothing to reclaim means no rewrite, and
// something to reclaim means a rewrite even on a pass that shed nothing.
//
// FREE PAGES ARE PLANTED in the second, because an untouched file and a vacuumed one are
// indistinguishable unless the vacuum has something to reclaim. A table created and dropped
// leaves its pages on the freelist; VACUUM drives that count to zero and shrinks `page_count`
// with it.
// ---------------------------------------------------------------------------------------

/// A whole-number `PRAGMA` read through a connection of its own, so the measurement never depends
/// on the state of the connection the store is using.
fn pragma_i64(db: &Path, pragma: &str) -> i64 {
    rusqlite::Connection::open(db)
        .expect("open the event log")
        .query_row(&format!("PRAGMA {pragma}"), [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("read PRAGMA {pragma}: {e}"))
}

/// Leave roughly `pages` worth of reclaimable free pages in `db`: a table filled and dropped
/// releases its pages to the freelist, where they stay until something vacuums the file.
fn plant_free_pages(db: &Path, rows: u64) {
    let conn = rusqlite::Connection::open(db).expect("open the event log");
    conn.execute_batch(&format!(
        "CREATE TABLE junk(x BLOB);
         INSERT INTO junk(x)
           WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < {rows})
           SELECT randomblob(600) FROM c;
         DROP TABLE junk;"
    ))
    .expect("plant reclaimable free pages");
}

/// Every byte of `db` as it stands on disk. A VACUUM rewrites the whole file - at the very least
/// the header's change counter moves - so an unchanged byte string is the assertion that no
/// rewrite ran, which neither `page_count` nor `freelist_count` can make about a file that had
/// nothing to reclaim in the first place.
fn file_bytes(db: &Path) -> Vec<u8> {
    std::fs::read(db).unwrap_or_else(|e| panic!("read {}: {e}", db.display()))
}

#[test]
fn a_prune_with_nothing_to_reclaim_leaves_the_file_unrewritten() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("clean.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    // ONE recording per replay key: the clean log the shipped guidance describes, differing from
    // every duplicated fixture in this file by exactly the round count.
    seed_namespace(&backend, "clean", 1);
    // AND A FILE WITH NOTHING IN IT TO RECLAIM. The vacuum below is skipped because of THIS, not
    // because of the zero deletes, so the fixture has to establish it rather than assume it.
    backend
        .prune_derived_index(
            &Namespaced::prefix_for("clean"),
            &rigger::ingest::derived_index_identity(),
        )
        .expect("settle the fixture into a compact file");
    let free_before = pragma_i64(&db, "freelist_count");
    assert_eq!(
        free_before, 0,
        "the fixture must hold no reclaimable free page, or this pins the wrong reason for the \
         rewrite being skipped"
    );
    let bytes_before = file_bytes(&db);

    let pruned = prune_all_types(&backend, &Namespaced::prefix_for("clean"));
    assert_eq!(
        pruned.total_removed(),
        0,
        "the fixture holds no key twice, so nothing may be shed; got {:?}",
        pruned.removed
    );
    assert_eq!(
        pruned.reclaimed_bytes,
        Some(0),
        "a prune over a file with no free space reclaimed nothing, and that is a MEASUREMENT \
         rather than a measurement it could not take: `None` means `unmeasured` and would send an \
         operator looking for pages that land at some later checkpoint. Got {:?}",
        pruned.reclaimed_bytes
    );
    assert_eq!(
        pruned.compaction_error, None,
        "a compaction that never ran cannot have failed"
    );
    assert_eq!(
        file_bytes(&db),
        bytes_before,
        "a prune with nothing to reclaim must not rewrite the file: a VACUUM here would hold the \
         write lock for a full scan and stage a second copy of the log in the temporary directory \
         to reclaim not one page"
    );
}

#[test]
fn a_prune_that_shed_nothing_still_reclaims_the_free_space_the_file_is_holding() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("left-behind.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    // A CLEAN LOG - no key recorded twice, so this pass deletes nothing - over a file that IS
    // holding reclaimable space. That is the shape a prune whose reclamation failed leaves
    // behind, and the shape the failure report tells the operator to re-run out of.
    seed_namespace(&backend, "left-behind", 1);
    plant_free_pages(&db, 3_000);

    let pages_before = pragma_i64(&db, "page_count");
    let free_before = pragma_i64(&db, "freelist_count");
    assert!(
        free_before > 100,
        "the fixture must leave real free pages, or an untouched file and a vacuumed one look \
         identical; the freelist holds {free_before} page(s)"
    );

    let pruned = prune_all_types(&backend, &Namespaced::prefix_for("left-behind"));
    assert_eq!(
        pruned.total_removed(),
        0,
        "the fixture holds no key twice, so this must be the zero-delete pass; got {:?}",
        pruned.removed
    );
    assert!(
        pruned.reclaimed_bytes.is_some_and(|b| b > 0),
        "a pass that deleted nothing must still reclaim the space the FILE is holding, or the \
         space a failed reclamation left behind is unreclaimable through this command forever; \
         got {pruned:?}"
    );
    assert_eq!(
        pragma_i64(&db, "freelist_count"),
        0,
        "the rewrite must actually have run: VACUUM drives the freelist to zero"
    );
    assert!(
        pragma_i64(&db, "page_count") < pages_before,
        "and it must shrink the file it reclaimed from: {pages_before} page(s) before"
    );
}

// ---------------------------------------------------------------------------------------
// 23. THE COMMAND on the path the shipped guidance calls the expected one - what it costs.
//
// Section 22 pins the store primitive from both sides: nothing to reclaim means no rewrite, and
// something to reclaim means a rewrite even on a pass that shed nothing. This pins the same
// property of the COMMAND, which is what the operator documents describe and what an operator
// actually runs, and which is strictly more than that one call - `rigger reset --derived` parses
// its modes, resolves the store by walking up from the working directory, runs the spec-09
// identity migration over it, and only then prunes. A rewrite reintroduced anywhere along that
// path - a maintenance step added beside the prune, a compaction moved up into the command - is
// invisible to a test that calls the store directly, and it is the command's cost, not the
// primitive's, that the shipped sentence "leaves the file exactly as it found it" is a promise
// about.
//
// The cost is the reason this matters at all. On a file with nothing to reclaim the rewrite would
// hold the write lock for a full scan of the log, stage a COMPLETE second copy of the database in
// the temporary directory SQLite resolves - a different and typically much smaller filesystem
// than the one holding `.rigger/` - and reclaim not one page. Section 20 cannot see any of that:
// it compares rows and dates, and a full VACUUM preserves every row and every date, so a command
// that rewrote the entire log to reclaim nothing passes it.
//
// AND THE OTHER SIDE IS THE OPERATOR'S REMEDY. The failure report tells them re-running is safe;
// a re-run deletes nothing, so a command that rewrote only when its own pass deleted something
// would never reclaim that space again. The second test runs the shipped binary over exactly that
// log - clean, but holding free pages - and holds it to reclaiming them.
// ---------------------------------------------------------------------------------------

#[test]
fn the_command_does_not_rewrite_a_file_it_has_nothing_to_reclaim_from() {
    let dir = temp_project();
    let root = dir.path();
    // ONE recording per replay key - the clean log of section 20, differing from the duplicated
    // fixtures by exactly the round count.
    seed_project(root, 1);
    let db = event_log(root);
    // AND A FILE ALREADY COMPACT. The rewrite is skipped because there is nothing to reclaim, so
    // the fixture establishes that rather than assuming it: a first pass settles the file, and
    // the pass this test measures is the one after it.
    let (settle, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(
        ok,
        "settling the fixture must succeed; stdout {settle:?}, stderr {err:?}"
    );
    let free_before = pragma_i64(&db, "freelist_count");
    assert_eq!(
        free_before, 0,
        "the fixture must hold no reclaimable free page, or this pins the wrong reason for the \
         rewrite being skipped. Settling run was {settle:?}"
    );
    let bytes_before = file_bytes(&db);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(
        ok,
        "a log with nothing to shed is not an error; stdout {out:?}, stderr {err:?}"
    );
    assert!(
        out.contains("pruned 0 redundant derived-index event(s)"),
        "the fixture must be the zero-delete path this section is about; got {out:?}"
    );

    // WHAT THE COMMAND SAYS IT DID TO THE FILE, which is the operator's only view of the cost it
    // did not pay.
    assert!(
        out.contains("left exactly as it stands"),
        "the report must tell the operator the file was not rewritten, or a skipped compaction is \
         indistinguishable from one that ran; got {out:?}"
    );
    assert!(
        !out.contains("byte(s) on disk"),
        "a run that did not compact the file must not report bytes reclaimed from it; got {out:?}"
    );

    // AND WHAT IT ACTUALLY DID. A VACUUM rewrites the whole file, so an unchanged byte string is
    // the assertion that no rewrite ran anywhere in the command - not only in the prune, and not
    // only where a page count could have seen it.
    assert_eq!(
        file_bytes(&db),
        bytes_before,
        "the command must not rewrite a file it has nothing to reclaim from - the path the \
         shipped guidance calls the expected one is the one an operator runs most. Report was \
         {out:?}"
    );

    // THE COMMITTED DOCUMENTS PROMISE EXACTLY THIS COST, read from the bytes an operator opens so
    // the promise and the binary cannot drift apart with the renderer green.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for rel in SHIPPED_DOCS {
        let shipped = std::fs::read_to_string(manifest.join(rel))
            .unwrap_or_else(|e| panic!("the operator document {rel} must ship: {e}"));
        assert!(
            shipped.contains("leaves the file exactly as it found it"),
            "the committed {rel} must promise that a prune with nothing to shed does not rewrite \
             the log, or the run above is behaviour nobody was told to expect"
        );
    }
}

#[test]
fn the_command_reclaims_the_free_space_a_failed_reclamation_left_in_the_file() {
    let dir = temp_project();
    let root = dir.path();
    // THE STATE A FAILED RECLAMATION LEAVES: the duplication is already gone (so this pass deletes
    // nothing) and the space it freed is still sitting in the file.
    seed_project(root, 1);
    let db = event_log(root);
    plant_free_pages(&db, 3_000);

    let pages_before = pragma_i64(&db, "page_count");
    let free_before = pragma_i64(&db, "freelist_count");
    assert!(
        free_before > 100,
        "the fixture must leave real free pages, or an untouched file and a rewritten one look \
         identical; the freelist holds {free_before} page(s)"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(
        ok,
        "the re-run must succeed; stdout {out:?}, stderr {err:?}"
    );
    assert!(
        out.contains("pruned 0 redundant derived-index event(s)"),
        "the fixture must be the zero-delete re-run this test is about; got {out:?}"
    );
    assert!(
        out.contains("byte(s) on disk"),
        "the re-run the failure report calls safe must actually reclaim the space, or it is safe \
         only in the sense of being pointless; got {out:?}"
    );
    assert!(
        !out.contains("left exactly as it stands"),
        "a run that DID rewrite the file must not tell the operator it left it alone; got {out:?}"
    );
    assert_eq!(
        pragma_i64(&db, "freelist_count"),
        0,
        "VACUUM drives the freelist to zero: the rewrite must really have run. Report was {out:?}"
    );
    assert!(
        pragma_i64(&db, "page_count") < pages_before,
        "and the file must be smaller than the {pages_before} page(s) it held. Report was {out:?}"
    );
}

// ---------------------------------------------------------------------------------------
// 24. WHY A DEDUPLICATED LOG STILL HAD SOMETHING TO SHED - the sentence that keeps a correct
//     prune from reading as a broken dedup.
//
// The shipped guidance now tells an operator that a clean log prunes to zero and that this is
// expected. That sentence has a second edge: having just been told zero is normal, an operator
// who then sees a NON-zero prune on a log written since the ingest dedup existed reads it as the
// dedup having failed - and goes looking for a defect in the ingest path. It has not failed. A
// file whose content RETURNS to a generation the log already recorded (a revert, a branch switch,
// a checkout back) re-records its whole batch by design, because a dedup that suppressed an
// already-recorded key would strand the graph on the version the file has since moved past.
//
// So the explanation ships in BOTH consumer documents and prints from the BINARY, and this pins
// the two to each other. Nothing else does: the renderer's own tests assert the documents, and a
// unit test asserts the report function, but the shipped binary printing what the shipped
// document promises is a claim neither can make. Section 20 covers only the reverse direction of
// the OTHER clause - that the clean-log sentence is absent here.
// ---------------------------------------------------------------------------------------

/// The clause a prune that DID shed rows adds, split into the three things it has to convey, so a
/// rewording that drops one of them fails by naming which one.
const WHY_A_DEDUPLICATED_LOG_STILL_SHEDS: [(&str, &str); 3] = [
    (
        "say a non-zero count is not a broken dedup",
        "not a sign the ingest dedup is broken",
    ),
    (
        "name the shape that re-records a batch",
        "RETURNS to a generation the log already recorded",
    ),
    ("give that shape its ordinary name", "revert"),
];

#[test]
fn a_prune_that_shed_rows_explains_itself_the_way_the_shipped_documents_do() {
    let dir = temp_project();
    let root = dir.path();
    seed_project(root, 4);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "the prune must succeed; stdout {out:?}, stderr {err:?}");
    assert!(
        !out.contains("pruned 0 redundant derived-index event(s)"),
        "the fixture must actually shed rows, or this is a test of the other branch; got {out:?}"
    );
    for (fact, needle) in WHY_A_DEDUPLICATED_LOG_STILL_SHEDS {
        assert!(
            out.contains(needle),
            "a prune that shed rows must {fact} ({needle:?}), or an operator who was just told \
             that zero is the expected report reads this one as the ingest dedup having failed. \
             Got {out:?}"
        );
    }

    // THE OTHER DIRECTION, so the clause is a statement about THIS log rather than boilerplate:
    // the clean log does not carry it.
    let clean = temp_project();
    seed_project(clean.path(), 1);
    let (clean_out, err, ok) = run_rigger(clean.path(), &["reset", "--derived"]);
    assert!(
        ok,
        "the clean prune must succeed; stdout {clean_out:?}, stderr {err:?}"
    );
    for (fact, needle) in WHY_A_DEDUPLICATED_LOG_STILL_SHEDS {
        assert!(
            !clean_out.contains(needle),
            "a prune that shed NOTHING must not {fact} ({needle:?}): an explanation of \
             duplication printed where there was none tells the operator their clean log holds \
             something it does not. Got {clean_out:?}"
        );
    }

    // AND THE COMMITTED DOCUMENTS SAY THE SAME THING, in the tense a document is written in. The
    // command explains the prune in front of the operator; the document explains it before they
    // run anything - and if only one of the two carries the rule, the other teaches the misread
    // this clause exists to prevent.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for rel in SHIPPED_DOCS {
        let shipped = std::fs::read_to_string(manifest.join(rel))
            .unwrap_or_else(|e| panic!("the operator document {rel} must ship: {e}"));
        for needle in [
            "WHEN A DEDUPLICATED LOG STILL HAS SOMETHING TO SHED",
            "RETURNED to a generation the log had already recorded",
            "revert",
        ] {
            assert!(
                shipped.contains(needle),
                "the committed {rel} must carry the same rule the binary prints ({needle:?}), or \
                 the two consumer surfaces disagree about what a non-zero prune means"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// 25. THE NUMBER: what the command says it reclaimed, against what the file actually lost.
//
// Section 21 pins the arm where the reclamation could NOT be measured. The measured arm - the one
// an operator sees on an ordinary run, and the only one that prints a number - is asserted
// nowhere at the binary: the suites check `reclaimed_bytes.is_some()` on the store's own report,
// which says the field was populated, not that the figure is true of the file on disk.
//
// An untrue figure here is the specific dishonesty this criterion is about. The page-count delta
// is the LOGICAL size the database shrank by; it becomes bytes on disk only once the truncating
// checkpoint folds the write-ahead log back into the file, and a report that skipped that step
// would print a number the operator's own `ls` contradicts. So the number is held to two things
// at once: it equals the pages the file lost, and the file on disk really is that size afterwards
// - no `-wal` still holding the frames the number already counted as reclaimed.
// ---------------------------------------------------------------------------------------

/// The byte count out of the report's measured-reclamation clause: `reclaimed <n> byte(s) on
/// disk`.
fn reported_reclaimed_bytes(out: &str) -> u64 {
    let marker = "reclaimed ";
    let at = out
        .find(marker)
        .unwrap_or_else(|| panic!("the report must carry a measured reclamation; got {out:?}"))
        + marker.len();
    let rest = &out[at..];
    let end = rest
        .find(" byte(s) on disk")
        .unwrap_or_else(|| panic!("the reclamation must be reported in bytes; got {out:?}"));
    rest[..end]
        .parse()
        .unwrap_or_else(|e| panic!("the reclamation must be a number ({e}); got {out:?}"))
}

/// Bytes of `path` on disk, or 0 when it does not exist - the `-wal` is deleted on a clean close.
fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

#[test]
fn the_reclamation_the_command_reports_is_the_space_the_file_actually_lost() {
    let dir = temp_project();
    let root = dir.path();
    seed_project(root, 4);
    let db = event_log(root);
    // ROOM TO RECLAIM. The seeded duplication is a few small rows, which can free no whole page
    // at all - a run that honestly reports zero would leave this test asserting nothing. Planted
    // free pages make the reclamation a definite figure without changing what it means: the
    // vacuum reclaims the pages the file is not using, however they came to be free.
    plant_free_pages(&db, 3_000);

    let page_size = pragma_i64(&db, "page_size");
    let pages_before = pragma_i64(&db, "page_count");
    let wal = db.with_extension("db-wal");
    // WHAT AN OPERATOR MEASURES: the bytes the log occupies on disk before the command runs -
    // the main file plus whatever its write-ahead log is holding. This is deliberately NOT the
    // page-count arithmetic the implementation could do internally: a page count is the LOGICAL
    // size of the database, it includes pages that live only in an un-checkpointed `-wal`, and a
    // report computed from it can name a reclamation while the file on disk grew.
    let on_disk_before = file_len(&db) + file_len(&wal);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "the prune must succeed; stdout {out:?}, stderr {err:?}");
    assert!(
        !out.contains("pruned 0 redundant derived-index event(s)"),
        "the fixture must shed rows, or the compaction this section measures never runs; got \
         {out:?}"
    );

    let pages_after = pragma_i64(&db, "page_count");
    let on_disk_after = file_len(&db) + file_len(&wal);
    let reclaimed = reported_reclaimed_bytes(&out);
    assert!(
        pages_after < pages_before,
        "the compaction must actually shrink the log, or the figure below is a claim about \
         nothing: {pages_before} page(s) before, {pages_after} after. Report was {out:?}"
    );
    assert_eq!(
        reclaimed,
        on_disk_before - on_disk_after,
        "the reported reclamation must be the bytes the log actually lost on disk - \
         {on_disk_before} before the command and {on_disk_after} after it, main file plus \
         write-ahead log. A number that is not this one is a claim an operator can disprove with \
         `du`. Report was {out:?}"
    );

    // AND IT LANDED IN THE MAIN FILE. The number above is a delta over main-plus-`-wal`; this is
    // what says the freed space really left the pair rather than moving between them, which it
    // does only because the truncating checkpoint folded the write-ahead log back into the file.
    // If the frames were still in the `-wal`, the file would not be the size the pages say it is
    // - the exact case section 21 makes the command report as unmeasured instead.
    assert_eq!(
        file_len(&db),
        pages_after as u64 * page_size as u64,
        "the log on disk must be exactly the pages it now holds, or the reclamation was reported \
         as landed while the freed frames were still in the write-ahead log"
    );
    assert_eq!(
        file_len(&wal),
        0,
        "and nothing may be left in the write-ahead log the reclamation already counted as \
         reclaimed"
    );
}

// ---------------------------------------------------------------------------------------
// 26. THE FACT THE REPORT CANNOT INFER: whether the file was REWRITTEN at all.
//
// `PrunedDerived` now carries `compaction_ran` beside the counts and the byte figure, and it is
// the one field of that report a caller cannot work out from the others. Both of the states it
// separates report `Some(0)` bytes - a file that was left alone because it held nothing to
// reclaim, and a rewrite that ran and reclaimed nothing - and they are different things to tell
// an operator watching a compaction. Every consumer of this report outside the crate reads it
// through this struct, so the field is a contract of its own and not an implementation detail of
// the one caller that formats it today.
//
// WHAT IT MUST NOT BE is the thing it was, twice, in this unit's own history: `total_removed() ==
// 0`. That inference is wrong in exactly the case the shipped guidance sends an operator into. A
// reclamation that fails after the deletes have committed leaves a log that is already pruned and
// still holding the free pages, and the failure report tells them re-running is safe AND that it
// retries the reclamation. That re-run deletes nothing. Inferred from the delete count it would
// report a file left untouched while it was rewriting it, and a rewrite gated the same way would
// never run at all - so the remedy the command prints would be a sentence about nothing.
//
// Sections 22 and 23 pin the BEHAVIOUR from both sides (nothing to reclaim means no rewrite;
// something to reclaim means a rewrite even on a pass that shed nothing), at the store and
// through the binary, by watching the file's bytes and its freelist. This section pins the
// REPORT: that the flag an out-of-crate caller reads tracks that same behaviour, in the two
// directions and in the arm where the rewrite ran but its result has not landed on disk yet.
//
// The two passes in the first test shed EXACTLY THE SAME NOTHING - one clean log, no key recorded
// twice, pruned twice - so the delete count cannot be what they differ by. The only thing that
// changes between them is the free space in the file, which is the whole claim.
// ---------------------------------------------------------------------------------------

#[test]
fn the_rewrite_flag_follows_the_file_and_not_this_passs_delete_count() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("flag.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let prefix = Namespaced::prefix_for("flag");
    // ONE recording per replay key: every pass below sheds nothing, which is what makes the two
    // reports comparable at all.
    seed_namespace(&backend, "flag", 1);
    // AND A SETTLED FILE. The seeding itself leaves pages on the freelist, so the state this test
    // is about - a file with nothing to reclaim - has to be established rather than assumed.
    prune_all_types(&backend, &prefix);
    assert_eq!(
        pragma_i64(&db, "freelist_count"),
        0,
        "the fixture must start from a file holding no reclaimable page, or the two passes below \
         differ by something other than the free space in the file"
    );

    // PASS ONE: nothing deleted, and nothing in the file to reclaim.
    let bytes_before = file_bytes(&db);
    let skipped = prune_all_types(&backend, &prefix);
    assert_eq!(
        skipped.total_removed(),
        0,
        "the fixture holds no key twice, so nothing may be shed; got {:?}",
        skipped.removed
    );
    assert!(
        !skipped.compaction_ran,
        "a file holding no reclaimable page must be reported as NOT rewritten: the rewrite is the \
         most expensive thing this command does, and declining it is a fact the operator is owed \
         rather than one they infer from a zero. Got {skipped:?}"
    );
    assert_eq!(
        skipped.reclaimed_bytes,
        Some(0),
        "and the zero beside it is a MEASUREMENT - there was nothing to reclaim - never an \
         unmeasured reclamation; got {skipped:?}"
    );
    assert_eq!(
        skipped.compaction_error, None,
        "a rewrite that never ran cannot have failed; got {skipped:?}"
    );
    assert_eq!(
        file_bytes(&db),
        bytes_before,
        "and the flag must be TRUE OF THE FILE: a VACUUM rewrites every byte, so an unchanged \
         byte string is what says the report of a skipped rewrite describes a skipped rewrite"
    );

    // PASS TWO: the same log and the same zero deletes, over a file that is now holding free
    // space. This is the shape a reclamation that failed after its deletes committed leaves
    // behind, and the shape the failure report tells an operator to re-run out of.
    plant_free_pages(&db, 3_000);
    let pages_before = pragma_i64(&db, "page_count");
    let rewrote = prune_all_types(&backend, &prefix);
    assert_eq!(
        rewrote.total_removed(),
        skipped.total_removed(),
        "both passes must shed the same nothing, or the flag below could be following the delete \
         count after all; skipped {:?}, rewrote {:?}",
        skipped.removed,
        rewrote.removed
    );
    assert!(
        rewrote.compaction_ran,
        "a pass that deleted nothing over a file WITH space to reclaim rewrites it, and must say \
         so: this is the re-run the failure report calls the remedy, and a report that told the \
         operator their log was left alone would make that remedy read as having done nothing. \
         Got {rewrote:?}"
    );
    assert!(
        rewrote.reclaimed_bytes.is_some_and(|b| b > 0),
        "and it reclaimed real space, so the flag is not a constant; got {rewrote:?}"
    );
    assert_eq!(
        rewrote.compaction_error, None,
        "an uncontended rewrite over a writable file must not fail; got {rewrote:?}"
    );
    assert_eq!(
        pragma_i64(&db, "freelist_count"),
        0,
        "and again the flag must be TRUE OF THE FILE: VACUUM drives the freelist to zero, which \
         is what says a report of a rewrite describes a rewrite. Got {rewrote:?}"
    );
    assert!(
        pragma_i64(&db, "page_count") < pages_before,
        "the rewritten file must be smaller than the {pages_before} page(s) it held; got \
         {rewrote:?}"
    );
}

/// The arm where the rewrite RAN and its result has not reached the disk yet, which is the one an
/// inference from the byte count gets exactly backwards.
///
/// A truncating checkpoint declines while any reader still holds a snapshot of the write-ahead
/// log, so the freed frames stay in the `-wal` and the reclamation is reported as unmeasured -
/// `None`, the same value a failure reports. Read the flag off the bytes and this file, freshly
/// vacuumed, is indistinguishable from one nobody touched. It is the difference between telling
/// an operator "the pages land at the next checkpoint" and telling them their log was left
/// exactly as it stands, which is the sentence they would check their own `ls` against.
#[test]
fn a_declined_checkpoint_reports_the_rewrite_that_ran_rather_than_a_file_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("declined.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let prefix = Namespaced::prefix_for("declined");
    seed_namespace(&backend, "declined", 6);
    // ROOM TO RECLAIM, or the rewrite is honestly skipped and the checkpoint this test is about
    // is never asked for: the seeded duplication is a handful of small rows that can free no
    // whole page.
    plant_free_pages(&db, 3_000);
    let free_before = pragma_i64(&db, "freelist_count");
    assert!(
        free_before > 100,
        "the fixture must leave real free pages; the freelist holds {free_before} page(s)"
    );

    // A READER PARKED ON THE WRITE-AHEAD LOG - an open read transaction from a second connection,
    // which is what a second rigger process holds.
    let reader = rusqlite::Connection::open(&db).expect("open a second connection");
    reader
        .execute_batch("BEGIN")
        .expect("begin the reader's transaction");
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("take a read snapshot");

    let report = prune_all_types(&backend, &prefix);
    drop(reader);

    assert_eq!(
        report.reclaimed_bytes, None,
        "the premise of this test: the checkpoint was declined, so the reclamation is UNMEASURED. \
         Got {report:?}"
    );
    assert_eq!(
        report.compaction_error, None,
        "and it was declined, not failed - the vacuum itself succeeded under the parked reader; \
         got {report:?}"
    );
    assert!(
        report.compaction_ran,
        "the file WAS rewritten, and an unmeasured reclamation must not be reported as a file \
         left alone: `None` bytes says only that the figure could not be taken, so the flag is \
         the only thing separating a deferred reclamation from a rewrite that never ran. Got \
         {report:?}"
    );
    assert!(
        report.on_disk_measured,
        "and this store HAS a file, so the before-measurement was taken: that is what makes this \
         `None` a checkpoint a reader declined rather than a database with nothing on disk to \
         measure. The two produce the identical (no error, rewritten, no bytes) shape and this \
         flag is the only thing between them - see \
         a_store_with_no_file_behind_it_reports_the_reclamation_as_unmeasured for the other side \
         of the pair. Got {report:?}"
    );
    assert_eq!(
        pragma_i64(&db, "freelist_count"),
        0,
        "and the flag is TRUE OF THE FILE: the vacuum ran and drove the freelist to zero even \
         though its result had not landed on disk when the report was made. Got {report:?}"
    );
}

/// The edge of the on-disk measurement itself: a store with NO FILE behind it.
///
/// `Store::open(":memory:")` is a published entry point of this API (its own doc comment names
/// it), and the reclamation the prune reports is defined as the bytes the log lost ON DISK -
/// measured over the database file and its `-wal`, because that is the pair an operator's own
/// `du` adds up. An in-memory database has neither, so there is no measurement to take, and the
/// honest report is `None`: a `Some(0)` here would tell a caller that a measurement was taken
/// over a file that does not exist and found nothing. The distinction is invisible from inside
/// the crate's own file-backed suites, and it is the difference between "unmeasured" and "we
/// looked and the log lost nothing".
#[test]
fn a_store_with_no_file_behind_it_reports_the_reclamation_as_unmeasured() {
    let backend = Store::open(":memory:").expect("open an in-memory store");
    let prefix = Namespaced::prefix_for("memory");
    // ENOUGH DUPLICATION that the deletes free whole pages: the rewrite is triggered by the free
    // space in the database, so a fixture too small to free a page would exercise the skipped arm
    // instead of the measurement edge this test is about.
    seed_namespace(&backend, "memory", 400);

    let pruned = prune_all_types(&backend, &prefix);
    assert!(
        pruned.total_removed() > 0,
        "the fixture records each key 400 times, so the prune must shed the earlier recordings; \
         got {pruned:?}"
    );
    assert!(
        pruned.compaction_ran,
        "the deletes freed whole pages, so the rewrite ran: this test is about what the RUN \
         reports, not about the skipped arm. Got {pruned:?}"
    );
    assert_eq!(
        pruned.compaction_error, None,
        "the rewrite of an in-memory database must not fail; got {pruned:?}"
    );
    assert_eq!(
        pruned.reclaimed_bytes, None,
        "a database with no file behind it has no bytes ON DISK to have lost, so the reclamation \
         is UNMEASURED. `Some(0)` would claim a measurement was taken - the one value this field \
         reserves for a file that was really looked at and had nothing to give back. Got {pruned:?}"
    );
    assert!(
        !pruned.on_disk_measured,
        "and the report must SAY WHICH unmeasured this is. The triple above - no failure, the \
         file rewritten, no byte figure - is the identical shape a checkpoint declined by a \
         concurrent reader produces on a real file, and a consumer handed only that shape can \
         only guess between them. Guessing renders a concurrent reader that was never there and \
         promises pages at a checkpoint that will never move a byte onto a disk this database \
         does not use. This flag is the fact that separates them, and it is FALSE here because \
         there was no before-measurement to take. Got {pruned:?}"
    );
}

// ---------------------------------------------------------------------------------------
// 27. THE BYTE FIGURE AND THE MEASUREMENT IT CLAIMS TO BE.
//
// `on_disk_measured` is section 26's sibling, and it answers exactly one question for a consumer
// of this report: was the PAIR OF ON-DISK SIZES the reclamation is a difference of ever sampled
// at all? It exists because `reclaimed_bytes: None` has two causes that are invisible in the
// numbers - a truncating checkpoint a concurrent reader declined, and a database with no file
// behind it - and a consumer handed only "unmeasured" has to guess between them.
//
// It is only worth something if the byte figure beside it AGREES with it, and the two are set
// independently: the flag from the database's own path, the figure from whichever of the four
// compaction arms the pass happened to take. So their agreement is a property of the STRUCT, not
// of any one arm, and it is the property every reader of the report takes for granted: A BYTE
// FIGURE IS ONLY EVER REPORTED WHERE A MEASUREMENT WAS TAKEN.
//
// `Some(n)` is documented as MEASURED - the field's own text says it is the pair of sizes an
// operator's own `du` would add up, sampled before the deletes and again after the rewrite - and
// section 22 leans on `Some(0)` meaning precisely "the file was really looked at and had nothing
// to give back", which is the one thing that separates it from `None`. A `Some` standing beside
// `on_disk_measured: false` therefore says both things at once: a measurement was taken, and no
// measurement was taken. There is no reading of that pair, so a caller either believes the number
// - and reports a reclamation of a file that does not exist - or learns to distrust every number
// the report carries.
//
// Section 26 reaches the flag in the arm where the rewrite RAN: its result pending on a real
// file, its bytes never existing on a fileless one. The arm below is the OTHER one an
// out-of-crate caller reaches, and the one the shipped guidance calls the expected path - the
// rewrite SKIPPED, decided by the file's free pages and never by this pass's deletes. A fileless
// database reaches it exactly the way a file with nothing to reclaim does, so the two are driven
// here side by side as one shape with one difference, and the invariant is asserted over every
// report either of them produced rather than at the single point where it is easiest to see.
//
// Invisible from inside the crate: every file-backed suite has a file, so the flag is true
// throughout and the pair can never disagree.
// ---------------------------------------------------------------------------------------

#[test]
fn a_prune_reports_a_byte_figure_only_where_it_had_a_file_to_measure_one_over() {
    // THE CONTROL, over a real file: one recording per replay key and a settling pass first, so
    // the pass under test sheds nothing AND finds no free page. That is the skipped arm, where
    // the zero is a measurement that WAS taken - the reading section 22 depends on.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("measured.db");
    let on_disk = Store::open(db.to_str().unwrap()).expect("open a file-backed store");
    let on_disk_prefix = Namespaced::prefix_for("measured");
    seed_namespace(&on_disk, "measured", 1);
    let on_disk_settle = prune_all_types(&on_disk, &on_disk_prefix);
    assert_eq!(
        pragma_i64(&db, "freelist_count"),
        0,
        "the control must hold no reclaimable free page before the pass under test, or it is not \
         in the arm this section is about"
    );

    let measured = prune_all_types(&on_disk, &on_disk_prefix);
    assert!(
        !measured.compaction_ran,
        "the control is the SKIPPED arm: the file held nothing to reclaim, so it was not \
         rewritten. Got {measured:?}"
    );
    assert!(
        measured.on_disk_measured,
        "this store has a file, so the before-measurement was taken; got {measured:?}"
    );
    assert_eq!(
        measured.reclaimed_bytes,
        Some(0),
        "and over a file that was really looked at, ZERO IS THE MEASUREMENT - the whole meaning \
         of `Some(0)` here, and what the fileless case below must not borrow. Got {measured:?}"
    );

    // THE SAME SHAPE with the one thing that matters changed: no file behind the database. Same
    // seeding, same settling pass, same arm - so nothing but the missing file can account for a
    // difference in what the report says about the measurement.
    let fileless = Store::open(":memory:").expect("open an in-memory store");
    let fileless_prefix = Namespaced::prefix_for("fileless");
    seed_namespace(&fileless, "fileless", 1);
    let fileless_settle = prune_all_types(&fileless, &fileless_prefix);

    let unmeasured = prune_all_types(&fileless, &fileless_prefix);
    assert_eq!(
        unmeasured.total_removed(),
        0,
        "the fixture holds no key twice, so nothing may be shed; got {:?}",
        unmeasured.removed
    );
    assert!(
        !unmeasured.compaction_ran,
        "the fileless store must reach the SAME arm as the control, or the two are not one shape \
         with one difference; got {unmeasured:?}"
    );
    assert!(
        !unmeasured.on_disk_measured,
        "a database with no file behind it took no before-measurement, and the flag says so; got \
         {unmeasured:?}"
    );
    assert_eq!(
        unmeasured.reclaimed_bytes, None,
        "SO IT MAY NOT HAND BACK A BYTE FIGURE. `Some(0)` here claims a measurement over a file \
         that does not exist, and claims it standing beside the flag that says no measurement was \
         taken - the one combination this report has no reading for. The honest answer is the one \
         the field's own documentation already gives for a database with no file behind it: \
         unmeasured, told apart from a checkpoint a reader declined by the flag rather than by \
         the number. Got {unmeasured:?}"
    );

    // AND AS THE RULE IT IS, over every report either store produced here - the settling passes
    // included, which are the same arm reached from a different starting state. A cross-field
    // invariant asserted only where it is easiest to see is a rule that holds at one point.
    for (which, pruned) in [
        ("the file-backed settling pass", &on_disk_settle),
        ("the file-backed pass under test", &measured),
        ("the fileless settling pass", &fileless_settle),
        ("the fileless pass under test", &unmeasured),
    ] {
        assert!(
            pruned.reclaimed_bytes.is_none() || pruned.on_disk_measured,
            "{which} reported a byte figure without a measurement behind it. Every `Some` in this \
             field is defined as a difference of two sampled sizes, so one may only appear where \
             the sampling happened. Got {pruned:?}"
        );
    }
}
