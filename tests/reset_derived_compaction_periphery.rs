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
//!      other alone - `--runs` prints "the event log is untouched", the shipped `--derived`
//!      guidance says the graph is unaffected. Neither claim is reachable from a test that runs
//!      one mode against one store, so both are pinned here against a project whose event log AND
//!      context graph are populated, together with the composition that follows from them: running
//!      the two together lands EXACTLY the two effects, neither more nor less.
//!   8. **The run history, read back through the binary after a compaction.** The shipped guidance
//!      promises that the whole run history `rigger stats` reads survives the prune. Rows surviving
//!      in the table is not that promise: the run read-model rides the namespace-scoped GLOBAL read
//!      (a `LIKE`-filtered scan across the file), a different path from the `read_stream` the
//!      revision-cursor test drives, and it is the one an operator actually looks at.
//!
//! Plus the command's own flag registry at the edges the composition opened: each mode named at
//! most once, and the two modes composing in EITHER order.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::Projection;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::{PrunedDerived, Store};
use rigger::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

fn rigger_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rigger")
}

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
        .prune_derived_index(
            prefix,
            rigger::ingest::META_REPLAY_KEY,
            &rigger::ingest::DERIVED_INDEX_TYPES,
        )
        .expect("prune the derived index")
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
        .prune_derived_index("", rigger::ingest::META_REPLAY_KEY, &[])
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
            "no_such_metadata_key",
            &rigger::ingest::DERIVED_INDEX_TYPES,
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
            rigger::ingest::META_REPLAY_KEY,
            &reversed,
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
    assert!(empty.removed.is_empty() && empty.reclaimed_bytes == 0);
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
    let mut cmd = Command::new(rigger_bin());
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
/// So the shipped bytes are read directly and held to the four things an operator must know before
/// running it - WHAT IT KEEPS, WHAT IT COSTS, that the file shrinks, and that the two prunes
/// compose - plus the proof that these documents were rendered from the real context at all.
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

        let paragraph = shipped
            .lines()
            .find(|l| l.contains("rigger reset --derived"))
            .unwrap_or_else(|| panic!("the guidance must be a paragraph in {rel}"))
            .to_string();
        paragraphs.push((rel.to_string(), paragraph));
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
/// what "the graph is unaffected" has to mean - the file itself is rebuilt by the `--runs` vacuum,
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
/// other alone: `reset --runs` prints "the event log is untouched", and the shipped `--derived`
/// guidance says the graph is unaffected. Both claims are load-bearing precisely BECAUSE the modes
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

    // `--runs` PRUNES THE GRAPH AND ONLY THE GRAPH. Its own report promises the event log is
    // untouched, so every row keeps its bytes AND its numbering: a row that survived but was
    // renumbered or repositioned is not an untouched log.
    let (out, err, ok) = run_rigger(runs_only.path(), &["reset", "--runs"]);
    assert!(ok, "reset --runs must succeed; stderr: {err}\n{out}");
    let after_runs_log = raw_rows(&event_log(runs_only.path()));
    assert!(
        after_runs_log == seed_log,
        "reset --runs reports that the event log is untouched, so every row must survive \
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
    // graph is unaffected - which it can be, because every recording of a key folds to the same
    // rows, so dropping the superseded ones changes nothing the projection holds.
    let (out, err, ok) = run_rigger(derived_only.path(), &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    assert_eq!(
        graph_rows(&graph_db(derived_only.path())),
        seed_graph,
        "the shipped guidance says the graph is unaffected by --derived, so its live content must \
         be identical; the command said: {out:?}"
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
