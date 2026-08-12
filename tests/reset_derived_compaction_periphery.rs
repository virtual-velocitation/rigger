//! PERIPHERY (contract / API / integration) tests for spec 60, criterion 5 - the boundary
//! surface `rigger reset --derived` opened, tested from OUTSIDE the code that implements it.
//!
//! The criterion's own behaviour - which rows the prune keeps, what the file weighs afterwards,
//! what the command printed, the loud refusal on a backend that cannot compact - is pinned by
//! `tests/reset_derived_compaction.rs`. This file does not re-litigate any of it. It covers the
//! four boundaries that unit is judged at but which its own tests cannot reach from inside a
//! single-project, single-invocation view:
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
//!
//! Plus the command's own flag registry at the edges the composition opened: each mode named at
//! most once, and the two modes composing in EITHER order.

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::{PrunedDerived, Store};
use rigger::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision};
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
